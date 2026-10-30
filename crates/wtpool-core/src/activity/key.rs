//! The actor a record belongs to, and the file stem that names it.

use super::harness::HarnessName;
use super::ids::ActorId;

/// The longest head of a record stem: the longest harness name, its dash and the longest session
/// id.
const MAX_HEAD: usize = HarnessName::MAX_LEN + 1 + ActorId::MAX_LEN;

/// An actor of one harness: a session, or a subagent of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorKey {
    pub harness: HarnessName,
    pub session: ActorId,
    /// `None` for the session itself.
    pub agent: Option<ActorId>,
}

/// A record file stem, `<head>` for a session or `<head>.<agent>` for a subagent of it. The head
/// is `<harness>-<session>`, or the session id alone for [`HarnessName::LEGACY`]. The grammar names
/// no harness, so the records of every harness read alike.
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
        if self.harness.is_legacy() {
            self.session.to_string()
        } else {
            format!("{}-{}", self.harness, self.session)
        }
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

    fn key(harness: HarnessName, session: &str, agent: Option<&str>) -> ActorKey {
        ActorKey {
            harness,
            session: ActorId::try_from(session).unwrap(),
            agent: agent.map(|agent| ActorId::try_from(agent).unwrap()),
        }
    }

    #[rstest]
    #[case::session("s-1", Some(("s-1", None)))]
    #[case::subagent("s-1.a-2", Some(("s-1", Some("a-2"))))]
    #[case::head_past_an_id(&"a".repeat(65), Some((&*"a".repeat(65), None)))]
    #[case::longest_head(&"a".repeat(89), Some((&*"a".repeat(89), None)))]
    #[case::too_long_head(&"a".repeat(90), None)]
    #[case::too_long_agent(&format!("s.{}", "a".repeat(65)), None)]
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

    #[rstest]
    #[case::legacy(HarnessName::LEGACY, "s1", "s1.a1")]
    #[case::named(
        HarnessName::new("other-harness"),
        "other-harness-s1",
        "other-harness-s1.a1"
    )]
    fn stem__should_name_the_actor_and_its_session(
        #[case] harness: HarnessName,
        #[case] session_stem: &str,
        #[case] subagent_stem: &str,
    ) {
        // Given
        let subagent = key(harness.clone(), "s1", Some("a1"));

        // When
        let session = subagent.session_key();

        // Then
        assert_eq!(session, key(harness, "s1", None));
        assert_eq!(
            (subagent.stem(), session.stem()),
            (subagent_stem.to_owned(), session_stem.to_owned())
        );
        let of = |stem: &str| Stem::parse(stem).unwrap().of_session(&subagent);
        let read = [
            session_stem,
            &format!("{session_stem}.a9"),
            &format!("{session_stem}0"),
        ]
        .map(of);
        assert_eq!(read, [true, true, false]);
    }

    #[test]
    fn parse__should_read_the_longest_stem_of_any_harness() {
        // Given
        let longest = ActorId::try_from("a".repeat(ActorId::MAX_LEN)).unwrap();
        let longest_name = HarnessName::try_from("h".repeat(HarnessName::MAX_LEN)).unwrap();
        let keys = [HarnessName::LEGACY, longest_name].map(|harness| ActorKey {
            harness,
            session: longest.clone(),
            agent: Some(longest.clone()),
        });

        // When
        let parsed = keys.each_ref().map(|key| {
            let stem = key.stem();
            Stem::parse(&stem).is_some_and(|stem| stem.of_session(key))
        });

        // Then
        assert_eq!(parsed, [true, true]);
    }
}
