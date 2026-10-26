//! `wtpool hook policy`: the text a harness's hook prints to refuse the shell call its adapter
//! reads, judged by [`crate::policy`] on the facts of this machine and the call's repository.

use super::adapter::Adapter;
use crate::policy::{PolicyEnv, PolicyFacts, decide};

/// The text the hook of `adapter`'s harness prints for its raw `input`: the harness's refusal of
/// the shell call in it, else `None`.
pub fn judge<A: Adapter>(adapter: &A, input: &[u8], env: &PolicyEnv) -> Option<String> {
    let call = adapter.shell_call(input)?;
    let facts = PolicyFacts::gather(&call, env)?;
    let refusal = decide(&call, &facts)?;
    adapter.refusal(&refusal)
}

#[cfg(test)]
mod tests;
