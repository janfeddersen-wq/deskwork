//! UI components for Deskwork.

pub mod attachments;
pub mod chat;
pub mod input;
pub mod menu;
pub mod settings;
pub mod status;

// Theme-aware colors for the UI
pub mod colors {
    use eframe::egui::{Color32, Visuals};

    /// User message bubble background (same for both themes)
    pub const USER_BG: Color32 = Color32::from_rgb(59, 130, 246); // Blue

    /// Success green
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);

    /// Error red
    pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68);

    /// Get assistant message background based on theme
    pub fn assistant_bg(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(55, 55, 60)
        } else {
            Color32::from_rgb(240, 240, 245)
        }
    }

    /// Get thinking block background based on theme
    pub fn thinking_bg(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(45, 45, 50)
        } else {
            Color32::from_rgb(250, 245, 230) // Warm beige for light mode
        }
    }

    /// Get tool call background based on theme
    pub fn tool_bg(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(40, 40, 45)
        } else {
            Color32::from_rgb(235, 240, 250) // Light blue-gray
        }
    }

    /// Get muted text color based on theme
    pub fn muted(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(156, 163, 175)
        } else {
            Color32::from_rgb(100, 100, 110)
        }
    }

    /// Get primary text color based on theme
    pub fn text(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(229, 231, 235)
        } else {
            Color32::from_rgb(30, 30, 35)
        }
    }

    /// Get code block background based on theme
    pub fn code_bg(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(30, 30, 35)
        } else {
            Color32::from_rgb(245, 245, 250)
        }
    }

    /// Get border color based on theme
    pub fn border(visuals: &Visuals) -> Color32 {
        if visuals.dark_mode {
            Color32::from_rgb(70, 70, 75)
        } else {
            Color32::from_rgb(200, 200, 210)
        }
    }
}
