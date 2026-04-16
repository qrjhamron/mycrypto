//! Intent detection from LLM responses.
//!
//! Parses [COMMAND:action] tags from AI responses to trigger actions.

/// A detected command from an LLM response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedIntent {
    /// The command name (e.g., "pause", "close").
    pub command: String,
    /// Optional argument (e.g., "BTCUSDT" for close command).
    pub argument: Option<String>,
    /// The full matched tag for removal from display.
    pub full_match: String,
}

impl DetectedIntent {
    /// Create a new intent.
    pub fn new(
        command: impl Into<String>,
        argument: Option<String>,
        full_match: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            argument,
            full_match: full_match.into(),
        }
    }
}

/// Result of parsing a response for intents.
#[derive(Debug, Clone)]
pub struct ParsedResponse {
    /// The response text with command tags removed.
    pub display_text: String,
    /// Detected intents/commands.
    pub intents: Vec<DetectedIntent>,
}

/// Parse LLM response text for command tags.
///
/// Looks for patterns like:
/// - [COMMAND:pause]
/// - [COMMAND:close BTCUSDT]
/// - [COMMAND:risk 2.5]
pub fn parse_response(text: &str) -> ParsedResponse {
    let mut display_text = text.to_string();
    let mut intents = Vec::new();

    // Regex-like parsing without regex dependency
    // Find all [COMMAND:...] patterns
    let mut search_start = 0;
    while let Some(start) = display_text[search_start..].find("[COMMAND:") {
        let abs_start = search_start + start;
        if let Some(end) = display_text[abs_start..].find(']') {
            let abs_end = abs_start + end + 1;
            let full_match = &display_text[abs_start..abs_end];

            // Parse the command content
            let content = &full_match[9..full_match.len() - 1]; // Remove "[COMMAND:" and "]"
            let parts: Vec<&str> = content.splitn(2, ' ').collect();

            let command = parts[0].to_lowercase();
            let argument = parts.get(1).map(|s| s.trim().to_string());

            intents.push(DetectedIntent::new(
                command,
                argument,
                full_match.to_string(),
            ));

            search_start = abs_end;
        } else {
            break;
        }
    }

    // Remove command tags from display text
    for intent in &intents {
        display_text = display_text.replace(&intent.full_match, "");
    }

    // Clean up extra whitespace
    display_text = display_text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    ParsedResponse {
        display_text,
        intents,
    }
}

/// Check if a partial response might contain an incomplete command tag.
///
/// Used during streaming to avoid displaying partial tags.
pub fn has_incomplete_tag(text: &str) -> bool {
    // Check for opening bracket without closing
    if let Some(start) = text.rfind('[') {
        // If there's an opening [ after the last ], it might be incomplete
        let last_close = text.rfind(']').unwrap_or(0);
        if start > last_close {
            return true;
        }
    }
    false
}

/// Buffer for accumulating streaming tokens and detecting commands.
#[derive(Debug, Default)]
pub struct StreamBuffer {
    /// Accumulated text.
    buffer: String,
    /// Text already sent to display.
    displayed_len: usize,
}

impl StreamBuffer {
    /// Create a new stream buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a token to the buffer.
    pub fn push(&mut self, token: &str) {
        self.buffer.push_str(token);
    }

    /// Get text safe to display (excluding potential incomplete tags).
    pub fn safe_display_text(&self) -> &str {
        let text = &self.buffer[self.displayed_len..];

        // If we might have an incomplete tag, hold back
        if let Some(bracket_pos) = text.rfind('[') {
            &text[..bracket_pos]
        } else {
            text
        }
    }

    /// Mark text as displayed.
    pub fn mark_displayed(&mut self, len: usize) {
        self.displayed_len += len;
    }

    /// Finalize and get the complete parsed response.
    pub fn finalize(self) -> ParsedResponse {
        parse_response(&self.buffer)
    }

    /// Get the full accumulated buffer.
    pub fn full_text(&self) -> &str {
        &self.buffer
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let text = "I'll pause the agent for you. [COMMAND:pause]";
        let parsed = parse_response(text);

        assert_eq!(parsed.intents.len(), 1);
        assert_eq!(parsed.intents[0].command, "pause");
        assert!(parsed.intents[0].argument.is_none());
        assert!(!parsed.display_text.contains("[COMMAND"));
    }

    #[test]
    fn test_parse_command_with_argument() {
        let text = "Closing your BTC position. [COMMAND:close BTCUSDT]";
        let parsed = parse_response(text);

        assert_eq!(parsed.intents.len(), 1);
        assert_eq!(parsed.intents[0].command, "close");
        assert_eq!(parsed.intents[0].argument, Some("BTCUSDT".to_string()));
    }

    #[test]
    fn test_parse_multiple_commands() {
        let text = "Let me show you the status [COMMAND:status] and then pause [COMMAND:pause]";
        let parsed = parse_response(text);

        assert_eq!(parsed.intents.len(), 2);
        assert_eq!(parsed.intents[0].command, "status");
        assert_eq!(parsed.intents[1].command, "pause");
    }

    #[test]
    fn test_parse_no_commands() {
        let text = "Bitcoin is trading at $67,500 today.";
        let parsed = parse_response(text);

        assert!(parsed.intents.is_empty());
        assert_eq!(parsed.display_text, text);
    }

    #[test]
    fn test_incomplete_tag_detection() {
        assert!(has_incomplete_tag("Hello [COMMAND:"));
        assert!(has_incomplete_tag("Test [COM"));
        assert!(!has_incomplete_tag("Test [COMMAND:pause]"));
        assert!(!has_incomplete_tag("No brackets here"));
    }

    #[test]
    fn test_stream_buffer() {
        let mut buffer = StreamBuffer::new();

        buffer.push("Hello ");
        buffer.push("[COMMAND:pause]");
        buffer.push(" world");

        let parsed = buffer.finalize();
        assert_eq!(parsed.intents.len(), 1);
        assert_eq!(parsed.intents[0].command, "pause");
    }

    #[test]
    fn test_stream_buffer_safe_display() {
        let mut buffer = StreamBuffer::new();

        buffer.push("Hello [");
        let safe = buffer.safe_display_text();
        assert_eq!(safe, "Hello ");

        buffer.push("COMMAND:pause]");
        let safe = buffer.safe_display_text();
        assert!(safe.contains("Hello"));
    }
}
