//! Shared types used across all wizard step modules.
//!
//! These are the primitive building blocks (step results, text input fields)
//! that every step module imports and uses. Keeping them in one place avoids
//! circular module dependencies.

/// The outcome returned by a step's `handle_input` function after processing a key event.
///
/// The wizard event loop inspects this to decide what to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// Stay on the current step -- the key was consumed but no navigation occurred.
    Continue,
    /// Advance to the next wizard step.
    Next,
    /// Return to the previous wizard step.
    Back,
    /// The user wants to quit the wizard. The caller should show a confirmation dialog.
    Quit,
}

/// A single editable text input field rendered inside a wizard step.
///
/// Each field carries its own validation function and error message so that
/// the step renderer can display inline error feedback without needing to
/// know the validation rules.
pub struct InputField {
    /// Label displayed to the left of the field.
    pub label: String,
    /// Current value entered by the user.
    pub value: String,
    /// Placeholder text shown when `value` is empty.
    pub placeholder: String,
    /// Cursor position within `value` (byte index, clamped to `value.len()`).
    pub cursor_pos: usize,
    /// Optional validation function. Returns `Some(error_message)` on invalid input.
    pub validator: Option<Box<dyn Fn(&str) -> Option<String>>>,
    /// Most recent validation error, or `None` if the field is valid or unvalidated.
    pub error: Option<String>,
}

impl InputField {
    /// Create a new input field with a label and placeholder. No validator.
    pub fn new(label: impl Into<String>, placeholder: impl Into<String>) -> Self {
        InputField {
            label: label.into(),
            value: String::new(),
            placeholder: placeholder.into(),
            cursor_pos: 0,
            validator: None,
            error: None,
        }
    }

    /// Create a new input field pre-populated with an initial value.
    pub fn with_value(
        label: impl Into<String>,
        value: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        let value = value.into();
        let cursor_pos = value.len();
        InputField {
            label: label.into(),
            value,
            placeholder: placeholder.into(),
            cursor_pos,
            validator: None,
            error: None,
        }
    }

    /// Attach a validator to this field.
    ///
    /// The validator receives the current value and returns an error message if
    /// the value is invalid, or `None` if it is acceptable.
    pub fn with_validator(mut self, f: impl Fn(&str) -> Option<String> + 'static) -> Self {
        self.validator = Some(Box::new(f));
        self
    }

    /// Insert `ch` at the current cursor position and advance the cursor.
    pub fn insert_char(&mut self, ch: char) {
        self.value.insert(self.cursor_pos, ch);
        self.cursor_pos += ch.len_utf8();
        self.validate();
    }

    /// Delete the character immediately before the cursor (backspace semantics).
    ///
    /// Does nothing if the cursor is at the start of the string.
    pub fn delete_char_before(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        // Find the start of the previous character (UTF-8 safe).
        let prev = self.value[..self.cursor_pos]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.value.drain(prev..self.cursor_pos);
        self.cursor_pos = prev;
        self.validate();
    }

    /// Move the cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        self.cursor_pos = self.value[..self.cursor_pos]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    /// Move the cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor_pos >= self.value.len() {
            return;
        }
        let ch = self.value[self.cursor_pos..].chars().next().unwrap();
        self.cursor_pos += ch.len_utf8();
    }

    /// Run the attached validator (if any) and update `self.error`.
    pub fn validate(&mut self) {
        if let Some(ref v) = self.validator {
            self.error = v(&self.value);
        }
    }

    /// Return the display value: the user-entered value, or the placeholder if empty.
    #[allow(dead_code)]
    pub fn display_value(&self) -> &str {
        if self.value.is_empty() {
            &self.placeholder
        } else {
            &self.value
        }
    }

    /// Return the effective value for plan building.
    ///
    /// If the user left the field empty, the placeholder is used as the default.
    pub fn effective_value(&self) -> &str {
        if self.value.is_empty() {
            &self.placeholder
        } else {
            &self.value
        }
    }
}

impl std::fmt::Debug for InputField {
    /// Formats the input field for debugging, omitting the validator closure.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputField")
            .field("label", &self.label)
            .field("value", &self.value)
            .field("cursor_pos", &self.cursor_pos)
            .field("error", &self.error)
            .finish()
    }
}
