use deepseek_client::types::ChatMessage;

// ─── Immutable Prefix ─────────────────────────────────────────────────────────

/// Holds the system prompt and generates the initial system message.
/// Once created, the prefix never changes — ensuring the system instructions
/// remain stable across the entire conversation.
#[derive(Debug, Clone)]
pub struct ImmutablePrefix {
    system_prompt: String,
}

impl ImmutablePrefix {
    /// Create a new prefix with the given system prompt.
    pub fn new(system_prompt: String) -> Self {
        Self { system_prompt }
    }

    /// Returns the prefix messages (currently just the system message).
    pub fn messages(&self) -> Vec<ChatMessage> {
        vec![ChatMessage::system(&self.system_prompt)]
    }

    /// The raw system prompt string (verbatim). Used by context compaction
    /// to reuse the exact cached prefix bytes in the fold summary call.
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

// ─── Append-Only Log ──────────────────────────────────────────────────────────

/// An append-only conversation log that stores user and assistant messages.
/// Messages can only be added, never removed — preserving full history.
#[derive(Debug, Clone, Default)]
pub struct AppendOnlyLog {
    messages: Vec<ChatMessage>,
}

impl AppendOnlyLog {
    /// Append a message to the log.
    pub fn append(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    /// Returns the number of messages in the log.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns true if the log contains no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Returns a slice of all messages in the log.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Replace the entire log with `replacement`. Used by compaction to
    /// substitute [summary_msg, ...recent_tail] for the full history.
    ///
    /// This is the only destructive operation on the log — it exists solely
    /// for the ContextManager fold path.
    pub fn compact_in_place(&mut self, replacement: Vec<ChatMessage>) {
        self.messages = replacement;
    }
}
