//! The actor a record belongs to, and the file stem that names it (H17, H25).

use super::harness::Harness;
use super::ids::ActorId;

/// The longest head of a record stem. H25's `<harness>-` prefix counts against it, as the stem
/// grammar of other harnesses is still open.
const MAX_HEAD: usize = 64;

/// An actor of one harness: a session, or a subagent of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorKey {
    pub harness: Harness,
    pub session: ActorId,
    /// `None` for the session itself.
    pub agent: Option<ActorId>,
}

/// A record file stem, `<head>` for a session or `<head>.<agent>` for a subagent of it (H17). The
/// head is the session id after its harness's prefix (H25). The grammar names no harness, so the
/// records of every harness read alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stem<'a> {
    pub head: &'a str,
    pub agent: Option<&'a str>,
}

impl ActorKey {
    /// The stem of the actor's record file.
    #[must_use]
    pub fn stem(&self) -> String {
        let head = self.head();
        match &self.agent {
            Some(agent) => format!("{head}.{agent}"),
            None => head,
        }
    }

    /// The key of the session this actor belongs to.
    #[must_use]
    pub fn session_key(&self) -> Self {
        Self {
            agent: None,
            ..self.clone()
        }
    }

    /// The head of every stem of the actor's session.
    fn head(&self) -> String {
        format!("{}{}", self.harness.stem_prefix(), self.session)
    }
}

impl<'a> Stem<'a> {
    /// The stem `stem`, when its head is 1 to `MAX_HEAD` ASCII letters, digits and dashes and
    /// its agent, if any, an actor id.
    #[must_use]
    pub fn parse(stem: &'a str) -> Option<Self> {
        let (head, agent) = match stem.split_once('.') {
            Some((head, agent)) => (head, Some(agent)),
            None => (stem, None),
        };
        let head_valid = (1..=MAX_HEAD).contains(&head.len())
            && head.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let agent_valid = agent.is_none_or(|agent| ActorId::try_from(agent).is_ok());
        (head_valid && agent_valid).then_some(Self { head, agent })
    }

    /// `true` for a stem of a record of `actor`'s session: the session's own or a subagent's.
    #[must_use]
    pub fn of_session(&self, actor: &ActorKey) -> bool {
        self.head == actor.head()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn key(session: &str, agent: Option<&str>) -> ActorKey {
        ActorKey {
            harness: Harness::ClaudeCode,
            session: ActorId::try_from(session).unwrap(),
            agent: agent.map(|agent| ActorId::try_from(agent).unwrap()),
        }
    }

    #[rstest]
    #[case::session("s-1", Some(("s-1", None)))]
    #[case::subagent("s-1.a-2", Some(("s-1", Some("a-2"))))]
    #[case::longest_id(&"a".repeat(64), Some((&*"a".repeat(64), None)))]
    #[case::too_long_id(&"a".repeat(65), None)]
    #[case::three_ids("a.b.c", None)]
    #[case::empty_id("", None)]
    #[case::empty_agent_id("s.", None)]
    #[case::empty_head(".a", None)]
    #[case::underscore("s_1", None)]
    #[case::unicode("sé", None)]
    fn parse__should_read_session_and_subagent_stems(
        #[case] stem: &str,
        #[case] expected: Option<(&str, Option<&str>)>,
    ) {
        // When
        let found = Stem::parse(stem);

        // Then
        assert_eq!(found.map(|stem| (stem.head, stem.agent)), expected);
    }

    #[test]
    fn stem__should_name_the_actor_and_its_session() {
        // Given
        let subagent = key("s1", Some("a1"));

        // When
        let session = subagent.session_key();

        // Then
        assert_eq!(session, key("s1", None));
        assert_eq!(
            (subagent.stem(), session.stem()),
            ("s1.a1".to_owned(), "s1".to_owned())
        );
        let of = |stem: &str| Stem::parse(stem).unwrap().of_session(&subagent);
        assert_eq!((of("s1"), of("s1.a9"), of("s10")), (true, true, false));
    }
}
