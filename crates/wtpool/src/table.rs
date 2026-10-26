//! The table of `wtpool ls` and `wtpool setup`: its columns, colours and fit to the terminal.

use wtpool_core::{Role, Row, State, Uncommitted, Unreferenced, Upstream};

use crate::text::visible;

/// How [`render`] lays the table out.
#[derive(Debug, Clone, Copy, Default)]
pub struct Style {
    pub colour: bool,
    /// Shorten the branch column so rows fit this many columns.
    pub max_width: Option<usize>,
}

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";
const BOLD_GREEN: &str = "\x1b[1m\x1b[32m";

const GAP: &str = "  ";
const HEADER: [&str; 4] = ["WORKTREE", "BRANCH", "ROLE", "STATE"];
const ELLIPSIS: char = '…';

/// A cell is a run of spans separated by single spaces.
#[derive(Debug, Clone)]
struct Cell(Vec<Span>);

impl Cell {
    fn plain(text: String) -> Self {
        Self(vec![Span::new(text, "")])
    }

    fn width(&self) -> usize {
        let gaps = self.0.len().saturating_sub(1);
        self.0
            .iter()
            .map(|s| s.text.chars().count())
            .fold(gaps, usize::saturating_add)
    }

    fn render(&self, colour: bool, out: &mut String) {
        for (i, span) in self.0.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            if colour && !span.paint.is_empty() {
                out.push_str(span.paint);
                out.push_str(&span.text);
                out.push_str(RESET);
            } else {
                out.push_str(&span.text);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Span {
    text: String,
    paint: &'static str,
}

impl Span {
    fn new(text: String, paint: &'static str) -> Self {
        Self { text, paint }
    }
}

/// Renders the `wtpool ls` table. Column widths come from the data, so colour codes
/// never shift a column. With `max_width`, only the branch column is shortened. Worktree and
/// branch names show as [`visible`] shows them, measured after.
#[must_use]
pub fn render(rows: &[Row], style: Style) -> String {
    let cells: Vec<[Cell; 4]> = rows.iter().map(row_cells).collect();
    let mut widths = HEADER.map(|h| h.chars().count());
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.width());
        }
    }
    if let Some(max) = style.max_width {
        let gaps = GAP.len().saturating_mul(widths.len().saturating_sub(1));
        let total = widths.iter().copied().fold(gaps, usize::saturating_add);
        let excess = total.saturating_sub(max);
        widths[1] = widths[1].saturating_sub(excess).max(HEADER[1].len());
    }

    let mut out = String::new();
    let header = HEADER.map(|title| Cell::plain(title.to_owned()));
    if style.colour {
        out.push_str(BOLD);
    }
    render_line(&header, &widths, /* colour */ false, &mut out);
    if style.colour {
        out.push_str(RESET);
    }
    out.push('\n');
    for row in &cells {
        render_line(row, &widths, style.colour, &mut out);
        out.push('\n');
    }
    out
}

fn render_line(cells: &[Cell; 4], widths: &[usize; 4], colour: bool, out: &mut String) {
    let last = cells.len().saturating_sub(1);
    for (i, (cell, &width)) in cells.iter().zip(widths).enumerate() {
        if i > 0 {
            out.push_str(GAP);
        }
        let shortened;
        let cell = if cell.width() > width {
            shortened = shorten(cell, width);
            &shortened
        } else {
            cell
        };
        cell.render(colour, out);
        if i < last {
            out.extend(std::iter::repeat_n(' ', width.saturating_sub(cell.width())));
        }
    }
}

fn shorten(cell: &Cell, width: usize) -> Cell {
    let text: String = cell
        .0
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<&str>>()
        .join(" ");
    let mut short: String = text.chars().take(width.saturating_sub(1)).collect();
    short.push(ELLIPSIS);
    Cell::plain(short)
}

fn row_cells(row: &Row) -> [Cell; 4] {
    let branch = row.branch.as_ref().map_or_else(
        || "(detached)".to_owned(),
        |branch| visible(branch.as_str()),
    );
    let role = match row.role {
        Role::Foreground => Span::new("foreground".to_owned(), BOLD_GREEN),
        Role::Slot => Span::new("slot".to_owned(), DIM),
        Role::Unmanaged => Span::new("unmanaged".to_owned(), YELLOW),
    };
    let State {
        dirty,
        upstream,
        unreferenced,
        operation,
    } = row.state;
    let mut state = Vec::new();
    match dirty {
        Uncommitted::Files(0) => {}
        Uncommitted::Files(count) => state.push(Span::new(format!("dirty:{count}"), YELLOW)),
        Uncommitted::Unknown => state.push(Span::new("dirty:?".to_owned(), YELLOW)),
    }
    match upstream {
        Upstream::Counted { ahead, behind } => {
            if ahead != 0 {
                state.push(Span::new(format!("ahead:{ahead}"), GREEN));
            }
            if behind != 0 {
                state.push(Span::new(format!("behind:{behind}"), RED));
            }
        }
        Upstream::Unknown => state.push(Span::new("upstream:?".to_owned(), YELLOW)),
    }
    match unreferenced {
        Unreferenced::Commits(0) => {}
        Unreferenced::Commits(count) => {
            state.push(Span::new(format!("unreferenced:{count}"), YELLOW));
        }
        Unreferenced::Unknown => state.push(Span::new("unreferenced:?".to_owned(), YELLOW)),
    }
    if let Some(label) = operation.label() {
        state.push(Span::new(label.to_owned(), RED));
    }
    if state.is_empty() {
        state.push(Span::new("clean".to_owned(), DIM));
    }
    [
        Cell::plain(visible(&row.worktree)),
        Cell::plain(branch),
        Cell(vec![role]),
        Cell(state),
    ]
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use wtpool_core::{BranchName, Operation};

    use super::*;

    const LONG: &str = "1234-check-widget-cache-health-before-each-deploy";

    fn must_row(worktree: &str, branch: Option<&str>, role: Role, state: State) -> Row {
        Row {
            worktree: worktree.to_owned(),
            branch: branch.map(|b| BranchName::try_new(b).unwrap()),
            role,
            state,
        }
    }

    fn pool() -> Vec<Row> {
        vec![
            must_row("app", Some(LONG), Role::Foreground, State::default()),
            must_row(
                "app-wt1",
                Some("build-widget-cache-generation-per-region"),
                Role::Slot,
                State {
                    dirty: Uncommitted::Files(2),
                    upstream: Upstream::Counted {
                        ahead: 1,
                        behind: 0,
                    },
                    ..State::default()
                },
            ),
            must_row("app-release", None, Role::Unmanaged, State::default()),
        ]
    }

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[rstest]
    #[case::no_width(None, &[
        "WORKTREE     BRANCH                                             ROLE        STATE",
        "app          1234-check-widget-cache-health-before-each-deploy  foreground  clean",
        "app-wt1      build-widget-cache-generation-per-region           slot        dirty:2 ahead:1",
        "app-release  (detached)                                         unmanaged   clean",
    ])]
    #[case::width_the_rows_fit(Some(200), &[
        "WORKTREE     BRANCH                                             ROLE        STATE",
        "app          1234-check-widget-cache-health-before-each-deploy  foreground  clean",
        "app-wt1      build-widget-cache-generation-per-region           slot        dirty:2 ahead:1",
        "app-release  (detached)                                         unmanaged   clean",
    ])]
    #[case::narrow_width_shortens_only_the_branch(Some(60), &[
        "WORKTREE     BRANCH              ROLE        STATE",
        "app          1234-check-widget…  foreground  clean",
        "app-wt1      build-widget-cach…  slot        dirty:2 ahead:1",
        "app-release  (detached)          unmanaged   clean",
    ])]
    #[case::tiny_width_keeps_the_branch_header(Some(10), &[
        "WORKTREE     BRANCH  ROLE        STATE",
        "app          1234-…  foreground  clean",
        "app-wt1      build…  slot        dirty:2 ahead:1",
        "app-release  (deta…  unmanaged   clean",
    ])]
    fn render__should_align_the_columns_and_shorten_only_the_branch_to_fit(
        #[case] max_width: Option<usize>,
        #[case] expected: &[&str],
    ) {
        // Given
        let rows = pool();

        // When
        let table = render(
            &rows,
            Style {
                colour: false,
                max_width,
            },
        );

        // Then
        assert_eq!(table, format!("{}\n", expected.join("\n")));
    }

    #[test]
    fn render__should_colour_the_header_without_shifting_a_column() {
        // Given
        let rows = pool();

        // When
        let coloured = render(
            &rows,
            Style {
                colour: true,
                max_width: Some(60),
            },
        );

        // Then
        assert!(
            coloured.starts_with(&format!("{BOLD}WORKTREE")),
            "{coloured}"
        );
        assert_eq!(
            strip_ansi(&coloured),
            render(
                &rows,
                Style {
                    colour: false,
                    max_width: Some(60)
                }
            )
        );
    }

    #[test]
    fn render__should_show_controls_in_names_as_escapes_and_align_after_them() {
        // Given
        let rows = [
            must_row(
                "w\u{200b}t",
                Some("x\u{9d}52;c;aGk=\u{9c}\u{202e}"),
                Role::Slot,
                State::default(),
            ),
            must_row("app", Some("main"), Role::Foreground, State::default()),
        ];

        // When
        let table = render(&rows, Style::default());

        // Then
        assert_eq!(
            table,
            "WORKTREE    BRANCH                              ROLE        STATE\n\
             w\\u{200B}t  x\\u{009D}52;c;aGk=\\u{009C}\\u{202E}  slot        clean\n\
             app         main                                foreground  clean\n"
        );
    }

    #[rstest]
    #[case::foreground(Role::Foreground, "foreground", BOLD_GREEN)]
    #[case::slot(Role::Slot, "slot", DIM)]
    #[case::unmanaged(Role::Unmanaged, "unmanaged", YELLOW)]
    fn render__should_name_and_colour_the_role(
        #[case] role: Role,
        #[case] label: &str,
        #[case] paint: &str,
    ) {
        // Given
        let rows = [must_row("w", None, role, State::default())];

        // When
        let plain = render(&rows, Style::default());
        let coloured = render(
            &rows,
            Style {
                colour: true,
                max_width: None,
            },
        );

        // Then
        assert_eq!(
            plain.lines().nth(1),
            Some(format!("w         (detached)  {label}  clean").as_str())
        );
        assert!(
            coloured.contains(&format!("{paint}{label}{RESET}")),
            "{coloured}"
        );
    }

    fn state(
        dirty: Uncommitted,
        upstream: Upstream,
        unreferenced: Unreferenced,
        operation: Operation,
    ) -> State {
        State {
            dirty,
            upstream,
            unreferenced,
            operation,
        }
    }

    const LEVEL: Upstream = Upstream::Counted {
        ahead: 0,
        behind: 0,
    };
    const NONE: Unreferenced = Unreferenced::Commits(0);
    const CLEAN: Uncommitted = Uncommitted::Files(0);

    #[rstest]
    #[case::clean(State::default(), &[("clean", DIM)])]
    #[case::dirty(
        state(Uncommitted::Files(2), LEVEL, NONE, Operation::Idle),
        &[("dirty:2", YELLOW)]
    )]
    #[case::unreadable_status(
        state(Uncommitted::Unknown, LEVEL, NONE, Operation::Idle),
        &[("dirty:?", YELLOW)]
    )]
    #[case::ahead(
        state(CLEAN, Upstream::Counted { ahead: 1, behind: 0 }, NONE, Operation::Idle),
        &[("ahead:1", GREEN)]
    )]
    #[case::behind(
        state(CLEAN, Upstream::Counted { ahead: 0, behind: 3 }, NONE, Operation::Idle),
        &[("behind:3", RED)]
    )]
    #[case::uncounted_upstream(
        state(CLEAN, Upstream::Unknown, NONE, Operation::Idle),
        &[("upstream:?", YELLOW)]
    )]
    #[case::unreferenced(
        state(CLEAN, LEVEL, Unreferenced::Commits(2), Operation::Idle),
        &[("unreferenced:2", YELLOW)]
    )]
    #[case::uncounted_unreferenced(
        state(CLEAN, LEVEL, Unreferenced::Unknown, Operation::Idle),
        &[("unreferenced:?", YELLOW)]
    )]
    #[case::rebase(state(CLEAN, LEVEL, NONE, Operation::Rebase), &[("rebase", RED)])]
    #[case::am(state(CLEAN, LEVEL, NONE, Operation::Am), &[("am", RED)])]
    #[case::merge(state(CLEAN, LEVEL, NONE, Operation::Merge), &[("merge", RED)])]
    #[case::cherry_pick(state(CLEAN, LEVEL, NONE, Operation::CherryPick), &[("cherry-pick", RED)])]
    #[case::revert(state(CLEAN, LEVEL, NONE, Operation::Revert), &[("revert", RED)])]
    #[case::bisect(state(CLEAN, LEVEL, NONE, Operation::Bisect), &[("bisect", RED)])]
    #[case::unknown_operation(
        state(CLEAN, LEVEL, NONE, Operation::Unknown),
        &[("operation:?", RED)]
    )]
    #[case::every_token_in_order(
        state(
            Uncommitted::Files(1),
            Upstream::Counted { ahead: 2, behind: 3 },
            Unreferenced::Commits(4),
            Operation::Merge
        ),
        &[("dirty:1", YELLOW), ("ahead:2", GREEN), ("behind:3", RED), ("unreferenced:4", YELLOW), ("merge", RED)]
    )]
    fn render__should_show_and_colour_each_state_token(
        #[case] state: State,
        #[case] tokens: &[(&str, &str)],
    ) {
        // Given
        let rows = [must_row("w", Some("b"), Role::Slot, state)];

        // When
        let plain = render(&rows, Style::default());
        let coloured = render(
            &rows,
            Style {
                colour: true,
                max_width: None,
            },
        );

        // Then
        let words: Vec<&str> = tokens.iter().map(|(word, _)| *word).collect();
        assert_eq!(
            plain.lines().nth(1),
            Some(format!("w         b       slot  {}", words.join(" ")).as_str())
        );
        let painted: Vec<String> = tokens
            .iter()
            .map(|(word, paint)| format!("{paint}{word}{RESET}"))
            .collect();
        assert!(
            coloured.ends_with(&format!("{}\n", painted.join(" "))),
            "{coloured:?}"
        );
    }
}
