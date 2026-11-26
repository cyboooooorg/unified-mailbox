//! Random bytes for secrets (bearer tokens, pairing codes), injected so
//! that use cases can be tested with predictable values.

/// Fills buffers with cryptographically secure random bytes.
///
/// An implementation that cannot draw random bytes must panic rather than
/// return a predictable buffer.
pub trait SecretGenerator: Send + Sync {
    fn fill(&self, buf: &mut [u8]);
}
