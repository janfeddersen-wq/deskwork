//! Markdown rendering primitives for chat responses.

use eframe::egui::{self, RichText, Stroke};
use streamdown_parser::{InlineElement, ListBullet, ParseEvent, Parser};

use crate::ui::colors;

#[derive(Debug, Clone, Default)]
pub struct MarkdownRenderState {
    source: String,
    model: MarkdownRenderModel,
}

impl MarkdownRenderState {
    pub fn update(&mut self, content: &str) {
        if self.source == content {
            return;
        }

        self.model = parse_markdown_to_model(content);
        self.source.clear();
        self.source.push_str(content);
    }

    pub fn model(&self) -> &MarkdownRenderModel {
        &self.model
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkdownRenderModel {
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarkdownBlock {
    Paragraph(Vec<InlineSpan>),
    Heading {
        level: u8,
        content: Vec<InlineSpan>,
    },
    ListItem {
        indent: usize,
        bullet: ListBullet,
        content: Vec<InlineSpan>,
    },
    CodeBlock {
        language: Option<String>,
        lines: Vec<String>,
    },
    Blockquote(Vec<InlineSpan>),
    HorizontalRule,
    TableHeader {
        cells: Vec<String>,
        alignments: Vec<TableAlignment>,
    },
    TableRow(Vec<String>),
    ThinkBlock(Vec<String>),
    Spacer,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InlineSpan {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        code: bool,
    },
    Link {
        text: String,
        url: String,
    },
}

#[derive(Debug, Default)]
struct BlockBuildState {
    paragraph: Vec<InlineSpan>,
    code_block: Option<CodeBlockBuilder>,
    think_lines: Option<Vec<String>>,
}

#[derive(Debug, Default)]
struct CodeBlockBuilder {
    language: Option<String>,
    lines: Vec<String>,
}

pub fn render_markdown(ui: &mut egui::Ui, state: &mut MarkdownRenderState, content: &str) {
    state.update(content);
    render_model(ui, state.model());
}

pub fn parse_markdown_to_model(content: &str) -> MarkdownRenderModel {
    let mut parser = Parser::new();
    let events = parser.parse_document(content);
    let table_alignments = extract_table_alignments(content);
    map_events_to_model_with_alignments(&events, &table_alignments)
}

fn map_events_to_model_with_alignments(
    events: &[ParseEvent],
    table_alignments: &[Vec<TableAlignment>],
) -> MarkdownRenderModel {
    let mut model = MarkdownRenderModel::default();
    let mut build = BlockBuildState::default();
    let mut table_alignment_idx = 0usize;
    let mut current_table_alignments: Option<Vec<TableAlignment>> = None;

    for event in events {
        match event {
            ParseEvent::Text(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::InlineCode(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: true,
                },
            ),
            ParseEvent::Bold(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: true,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::Italic(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: true,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::Underline(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: true,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::Strikeout(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: true,
                    code: false,
                },
            ),
            ParseEvent::BoldItalic(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: true,
                    italic: true,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::Footnote(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
            ParseEvent::Link { text, url } => {
                push_inline(
                    &mut build.paragraph,
                    InlineSpan::Link {
                        text: text.clone(),
                        url: url.clone(),
                    },
                );
            }
            ParseEvent::Image { alt, url } => push_inline(
                &mut build.paragraph,
                InlineSpan::Link {
                    text: format!("[image: {}]", alt),
                    url: url.clone(),
                },
            ),
            ParseEvent::InlineElements(elements) => {
                for element in elements {
                    push_inline(&mut build.paragraph, span_from_inline_element(element));
                }
            }
            ParseEvent::Newline => flush_paragraph(&mut model, &mut build),
            ParseEvent::EmptyLine => {
                flush_paragraph(&mut model, &mut build);
                model.blocks.push(MarkdownBlock::Spacer);
            }
            ParseEvent::Heading { level, content } => {
                flush_paragraph(&mut model, &mut build);
                model.blocks.push(MarkdownBlock::Heading {
                    level: *level,
                    content: parse_inline_spans(content),
                });
            }
            ParseEvent::ListItem {
                indent,
                bullet,
                content,
            } => {
                flush_paragraph(&mut model, &mut build);
                model.blocks.push(MarkdownBlock::ListItem {
                    indent: *indent,
                    bullet: *bullet,
                    content: parse_inline_spans(content),
                });
            }
            ParseEvent::ListEnd => flush_paragraph(&mut model, &mut build),
            ParseEvent::CodeBlockStart { language, .. } => {
                flush_paragraph(&mut model, &mut build);
                build.code_block = Some(CodeBlockBuilder {
                    language: language.clone(),
                    lines: Vec::new(),
                });
            }
            ParseEvent::CodeBlockLine(line) => {
                if let Some(block) = &mut build.code_block {
                    block.lines.push(line.clone());
                }
            }
            ParseEvent::CodeBlockEnd => {
                if let Some(block) = build.code_block.take() {
                    model.blocks.push(MarkdownBlock::CodeBlock {
                        language: block.language,
                        lines: block.lines,
                    });
                }
            }
            ParseEvent::BlockquoteStart { .. } => flush_paragraph(&mut model, &mut build),
            ParseEvent::BlockquoteLine(line) => {
                flush_paragraph(&mut model, &mut build);
                model
                    .blocks
                    .push(MarkdownBlock::Blockquote(parse_inline_spans(line)));
            }
            ParseEvent::BlockquoteEnd => flush_paragraph(&mut model, &mut build),
            ParseEvent::HorizontalRule => {
                flush_paragraph(&mut model, &mut build);
                model.blocks.push(MarkdownBlock::HorizontalRule);
            }
            ParseEvent::TableHeader(cells) => {
                flush_paragraph(&mut model, &mut build);
                if current_table_alignments.is_none() {
                    let alignments = table_alignments
                        .get(table_alignment_idx)
                        .cloned()
                        .unwrap_or_else(|| default_table_alignments(cells.len()));
                    current_table_alignments = Some(alignments);
                    table_alignment_idx += 1;
                }
                model.blocks.push(MarkdownBlock::TableHeader {
                    cells: cells.clone(),
                    alignments: current_table_alignments
                        .clone()
                        .unwrap_or_else(|| default_table_alignments(cells.len())),
                });
            }
            ParseEvent::TableRow(cells) => {
                flush_paragraph(&mut model, &mut build);
                model.blocks.push(MarkdownBlock::TableRow(cells.clone()));
            }
            ParseEvent::TableSeparator => {}
            ParseEvent::TableEnd => {
                current_table_alignments = None;
            }
            ParseEvent::ThinkBlockStart => {
                flush_paragraph(&mut model, &mut build);
                build.think_lines = Some(Vec::new());
            }
            ParseEvent::ThinkBlockLine(line) => {
                if let Some(lines) = &mut build.think_lines {
                    lines.push(line.clone());
                }
            }
            ParseEvent::ThinkBlockEnd => {
                if let Some(lines) = build.think_lines.take() {
                    model.blocks.push(MarkdownBlock::ThinkBlock(lines));
                }
            }
            ParseEvent::Prompt(text) => push_inline(
                &mut build.paragraph,
                InlineSpan::Text {
                    text: text.clone(),
                    bold: false,
                    italic: false,
                    underline: false,
                    strike: false,
                    code: false,
                },
            ),
        }
    }

    flush_paragraph(&mut model, &mut build);

    if let Some(code) = build.code_block.take() {
        model.blocks.push(MarkdownBlock::CodeBlock {
            language: code.language,
            lines: code.lines,
        });
    }

    if let Some(lines) = build.think_lines.take() {
        model.blocks.push(MarkdownBlock::ThinkBlock(lines));
    }

    model
}

fn push_inline(target: &mut Vec<InlineSpan>, span: InlineSpan) {
    target.push(span);
}

fn flush_paragraph(model: &mut MarkdownRenderModel, build: &mut BlockBuildState) {
    if build.paragraph.is_empty() {
        return;
    }

    model.blocks.push(MarkdownBlock::Paragraph(std::mem::take(
        &mut build.paragraph,
    )));
}

fn parse_inline_spans(content: &str) -> Vec<InlineSpan> {
    let mut parser = streamdown_parser::InlineParser::new();
    parser
        .parse(content)
        .iter()
        .map(span_from_inline_element)
        .collect()
}

fn span_from_inline_element(element: &InlineElement) -> InlineSpan {
    match element {
        InlineElement::Text(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
        },
        InlineElement::Bold(text) => InlineSpan::Text {
            text: text.clone(),
            bold: true,
            italic: false,
            underline: false,
            strike: false,
            code: false,
        },
        InlineElement::Italic(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: true,
            underline: false,
            strike: false,
            code: false,
        },
        InlineElement::BoldItalic(text) => InlineSpan::Text {
            text: text.clone(),
            bold: true,
            italic: true,
            underline: false,
            strike: false,
            code: false,
        },
        InlineElement::Underline(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: false,
            underline: true,
            strike: false,
            code: false,
        },
        InlineElement::Strikeout(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: false,
            underline: false,
            strike: true,
            code: false,
        },
        InlineElement::Code(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: true,
        },
        InlineElement::Link { text, url } => InlineSpan::Link {
            text: text.clone(),
            url: url.clone(),
        },
        InlineElement::Image { alt, url } => InlineSpan::Link {
            text: format!("[image: {}]", alt),
            url: url.clone(),
        },
        InlineElement::Footnote(text) => InlineSpan::Text {
            text: text.clone(),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
        },
    }
}

fn render_model(ui: &mut egui::Ui, model: &MarkdownRenderModel) {
    let mut idx = 0usize;
    while idx < model.blocks.len() {
        match &model.blocks[idx] {
            MarkdownBlock::Paragraph(spans) => {
                render_spans(ui, spans, 14.0);
                idx += 1;
            }
            MarkdownBlock::Heading { level, content } => {
                let size = match level {
                    1 => 24.0,
                    2 => 20.0,
                    3 => 18.0,
                    4 => 16.0,
                    _ => 15.0,
                };
                render_spans(ui, content, size);
                ui.add_space(2.0);
                idx += 1;
            }
            MarkdownBlock::ListItem {
                indent,
                bullet,
                content,
            } => {
                let bullet_text = match bullet {
                    ListBullet::Ordered(n) => format!("{}.", n),
                    ListBullet::PlusExpand => "+-".to_string(),
                    ListBullet::Dash | ListBullet::Asterisk | ListBullet::Plus => "•".to_string(),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((*indent as f32) * 8.0);
                    ui.label(RichText::new(bullet_text).size(14.0).strong());
                    render_spans(ui, content, 14.0);
                });
                idx += 1;
            }
            MarkdownBlock::CodeBlock { language, lines } => {
                let code_bg = colors::code_bg(ui.visuals());
                let border = colors::border(ui.visuals());
                egui::Frame::none()
                    .fill(code_bg)
                    .stroke(Stroke::new(1.0, border))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        if let Some(language) = language {
                            if !language.is_empty() {
                                ui.label(
                                    RichText::new(language)
                                        .size(11.0)
                                        .color(colors::muted(ui.visuals()))
                                        .italics(),
                                );
                            }
                        }

                        for line in lines {
                            ui.label(RichText::new(line).monospace().size(13.0));
                        }
                    });
                idx += 1;
            }
            MarkdownBlock::Blockquote(spans) => {
                render_blockquote(ui, spans);
                idx += 1;
            }
            MarkdownBlock::HorizontalRule => {
                render_horizontal_rule(ui);
                idx += 1;
            }
            MarkdownBlock::TableHeader { .. } | MarkdownBlock::TableRow(_) => {
                if let Some((table_region, next_idx)) = collect_table_region(&model.blocks, idx) {
                    render_table_region(ui, &table_region, idx);
                    idx = next_idx;
                } else {
                    idx += 1;
                }
            }
            MarkdownBlock::ThinkBlock(lines) => {
                let thinking_bg = colors::thinking_bg(ui.visuals());
                egui::Frame::none()
                    .fill(thinking_bg)
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        for line in lines {
                            ui.label(
                                RichText::new(line)
                                    .size(12.0)
                                    .italics()
                                    .color(colors::muted(ui.visuals())),
                            );
                        }
                    });
                idx += 1;
            }
            MarkdownBlock::Spacer => {
                ui.add_space(4.0);
                idx += 1;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TableRegion {
    header: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
    columns: usize,
    alignments: Vec<TableAlignment>,
}

fn collect_table_region(
    blocks: &[MarkdownBlock],
    start_idx: usize,
) -> Option<(TableRegion, usize)> {
    if start_idx >= blocks.len() {
        return None;
    }

    let mut idx = start_idx;
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<Vec<String>> = Vec::new();

    let mut alignments: Option<Vec<TableAlignment>> = None;

    while idx < blocks.len() {
        match &blocks[idx] {
            MarkdownBlock::TableHeader {
                cells,
                alignments: header_alignments,
            } => {
                if header.is_none() {
                    header = Some(cells.clone());
                    alignments = Some(header_alignments.clone());
                } else {
                    rows.push(cells.clone());
                }
                idx += 1;
            }
            MarkdownBlock::TableRow(cells) => {
                rows.push(cells.clone());
                idx += 1;
            }
            _ => break,
        }
    }

    if header.is_none() && rows.is_empty() {
        return None;
    }

    let columns = table_column_count(header.as_ref(), &rows);
    if columns == 0 {
        return None;
    }

    Some((
        TableRegion {
            header,
            rows,
            columns,
            alignments: normalize_table_alignments(
                alignments.unwrap_or_else(|| default_table_alignments(columns)),
                columns,
            ),
        },
        idx,
    ))
}

fn table_column_count(header: Option<&Vec<String>>, rows: &[Vec<String>]) -> usize {
    let mut columns = header.map(|h| h.len()).unwrap_or(0);
    for row in rows {
        columns = columns.max(row.len());
    }
    columns
}

fn default_table_alignments(columns: usize) -> Vec<TableAlignment> {
    vec![TableAlignment::Left; columns]
}

fn normalize_table_alignments(
    mut alignments: Vec<TableAlignment>,
    columns: usize,
) -> Vec<TableAlignment> {
    if alignments.len() > columns {
        alignments.truncate(columns);
        return alignments;
    }

    while alignments.len() < columns {
        alignments.push(TableAlignment::Left);
    }

    alignments
}

fn extract_table_alignments(content: &str) -> Vec<Vec<TableAlignment>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut idx = 0usize;
    let mut in_fenced_code = false;

    while idx + 1 < lines.len() {
        let current = lines[idx].trim();
        let next = lines[idx + 1].trim();

        if is_fence_marker(current) {
            in_fenced_code = !in_fenced_code;
            idx += 1;
            continue;
        }

        if in_fenced_code {
            idx += 1;
            continue;
        }

        if is_table_row_line(current) {
            if let Some(alignments) = parse_table_separator_alignments(next) {
                result.push(alignments);
                idx += 2;
                continue;
            }
        }

        idx += 1;
    }

    result
}

fn is_fence_marker(line: &str) -> bool {
    line.starts_with("```")
        || line.starts_with("~~~")
        || line.eq_ignore_ascii_case("<pre>")
        || line.eq_ignore_ascii_case("</pre>")
}

fn is_table_row_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2
}

fn parse_table_separator_alignments(line: &str) -> Option<Vec<TableAlignment>> {
    let trimmed = line.trim();
    if !is_table_row_line(trimmed) {
        return None;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let mut alignments = Vec::new();

    for cell in inner.split('|') {
        let token = cell.trim();
        if token.is_empty() || !token.chars().all(|ch| ch == '-' || ch == ':') {
            return None;
        }

        let has_dash = token.contains('-');
        if !has_dash {
            return None;
        }

        let align = if token.starts_with(':') && token.ends_with(':') {
            TableAlignment::Center
        } else if token.ends_with(':') {
            TableAlignment::Right
        } else {
            TableAlignment::Left
        };
        alignments.push(align);
    }

    if alignments.is_empty() {
        return None;
    }

    Some(alignments)
}

fn render_horizontal_rule(ui: &mut egui::Ui) {
    let local_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(local_width, 8.0), egui::Sense::hover());

    if let Some((start_offset, end_offset)) = horizontal_rule_x_bounds(rect.width(), 2.0) {
        let y = rect.center().y;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + start_offset, y),
                egui::pos2(rect.left() + end_offset, y),
            ],
            Stroke::new(1.0, colors::border(ui.visuals())),
        );
    }
}

fn horizontal_rule_x_bounds(local_width: f32, horizontal_padding: f32) -> Option<(f32, f32)> {
    if local_width <= 0.0 {
        return None;
    }

    let start = horizontal_padding.max(0.0);
    let end = (local_width - horizontal_padding).max(0.0);

    if end <= start {
        return None;
    }

    Some((start, end))
}

/// Known emoji-to-colored-text substitutions.
/// Returns a list of (text_segment, optional_color) pairs.
fn split_emoji_segments(text: &str) -> Vec<(String, Option<egui::Color32>)> {
    // Color constants for emoji substitutions
    const GREEN: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);
    const RED: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
    const YELLOW: egui::Color32 = egui::Color32::from_rgb(234, 179, 8);
    const BLUE: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
    const ORANGE: egui::Color32 = egui::Color32::from_rgb(249, 115, 22);

    // Map of emoji -> (replacement_char, color)
    let emoji_map: &[(&str, &str, egui::Color32)] = &[
        ("✅", "☑ ", GREEN),
        ("🟢", "● ", GREEN),
        ("❌", "✗ ", RED),
        ("🔴", "● ", RED),
        ("⚠️", "▲ ", ORANGE),
        ("⚠", "▲ ", ORANGE), // without variation selector
        ("🟡", "● ", YELLOW),
        ("🟠", "● ", ORANGE),
        ("🔵", "● ", BLUE),
        ("🟣", "● ", egui::Color32::from_rgb(168, 85, 247)),
        ("⛔", "⊘ ", RED),
        ("🚫", "⊘ ", RED),
        ("💚", "♥ ", GREEN),
        ("❤️", "♥ ", RED),
        ("❤", "♥ ", RED), // without variation selector
        ("🔶", "◆ ", ORANGE),
        ("🔷", "◆ ", BLUE),
        ("✓", "✓ ", GREEN),
        ("✗", "✗ ", RED),
    ];

    if text.is_empty() {
        return vec![(String::new(), None)];
    }

    let mut segments: Vec<(String, Option<egui::Color32>)> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Try to find the earliest emoji match
        let mut earliest_match: Option<(usize, &str, &str, egui::Color32)> = None;

        for &(emoji, replacement, color) in emoji_map {
            if let Some(pos) = remaining.find(emoji) {
                if earliest_match.is_none() || pos < earliest_match.unwrap().0 {
                    earliest_match = Some((pos, emoji, replacement, color));
                }
            }
        }

        match earliest_match {
            Some((pos, emoji, replacement, color)) => {
                // Push any text before the emoji
                if pos > 0 {
                    segments.push((remaining[..pos].to_string(), None));
                }
                // Push the colored replacement
                segments.push((replacement.to_string(), Some(color)));
                remaining = &remaining[pos + emoji.len()..];
            }
            None => {
                // No more emojis, push the rest
                segments.push((remaining.to_string(), None));
                break;
            }
        }
    }

    if segments.is_empty() {
        segments.push((String::new(), None));
    }

    segments
}

/// Build a galley for a table cell, rendering inline markdown (bold, italic, code, etc.)
/// and applying emoji color substitutions.
fn build_cell_galley(
    ui: &egui::Ui,
    spans: &[InlineSpan],
    base_color: egui::Color32,
    wrap_width: f32,
    font_id: &egui::FontId,
    bold_font_id: &egui::FontId,
    italic_font_id: &egui::FontId,
    code_font_id: &egui::FontId,
    code_bg: egui::Color32,
    is_header: bool,
) -> std::sync::Arc<egui::epaint::text::Galley> {
    use egui::text::{LayoutJob, TextFormat};

    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    for span in spans {
        match span {
            InlineSpan::Text {
                text,
                bold,
                italic,
                code,
                strike,
                underline,
            } => {
                // Split text into emoji and non-emoji segments for color handling
                let segments = split_emoji_segments(text);
                for segment in segments {
                    let (segment_text, emoji_color) = segment;
                    let chosen_font = if *code {
                        code_font_id.clone()
                    } else if *bold || is_header {
                        bold_font_id.clone()
                    } else if *italic {
                        italic_font_id.clone()
                    } else {
                        font_id.clone()
                    };

                    let text_color = emoji_color.unwrap_or(base_color);

                    let mut format = TextFormat {
                        font_id: chosen_font,
                        color: text_color,
                        strikethrough: if *strike {
                            egui::Stroke::new(1.0, text_color)
                        } else {
                            egui::Stroke::NONE
                        },
                        underline: if *underline {
                            egui::Stroke::new(1.0, text_color)
                        } else {
                            egui::Stroke::NONE
                        },
                        ..Default::default()
                    };

                    if *code {
                        format.background = code_bg;
                    }
                    if *bold || is_header {
                        // egui LayoutJob doesn't have a "bold" flag, but we can use a
                        // slightly larger font or rely on the font itself. Since we only
                        // have Inter-Regular (no bold weight), we approximate bold by
                        // using strong_text_color which makes it stand out.
                        format.color = if *bold {
                            ui.visuals().strong_text_color()
                        } else {
                            text_color
                        };
                    }
                    if *italic {
                        format.italics = true;
                    }

                    job.append(&segment_text, 0.0, format);
                }
            }
            InlineSpan::Link { text, .. } => {
                let format = TextFormat {
                    font_id: font_id.clone(),
                    color: egui::Color32::from_rgb(100, 149, 237), // cornflower blue
                    underline: egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 149, 237)),
                    ..Default::default()
                };
                job.append(text, 0.0, format);
            }
        }
    }

    ui.fonts(|fonts| fonts.layout_job(job))
}

fn render_table_region(ui: &mut egui::Ui, region: &TableRegion, _table_index: usize) {
    let border = colors::border(ui.visuals());
    let table_bg = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(39, 39, 44)
    } else {
        egui::Color32::from_rgb(250, 251, 255)
    };
    let header_bg = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(52, 52, 58)
    } else {
        egui::Color32::from_rgb(232, 235, 242)
    };
    let row_bg = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(44, 44, 49)
    } else {
        egui::Color32::from_rgb(245, 247, 252)
    };

    let table_width = ui.available_width().max(1.0);
    let column_widths = compute_table_column_widths(region.columns, table_width, 72.0);

    let mut rows: Vec<(&[String], bool)> = Vec::new();
    if let Some(header) = &region.header {
        rows.push((header.as_slice(), true));
    }
    for row in &region.rows {
        rows.push((row.as_slice(), false));
    }

    if rows.is_empty() {
        return;
    }

    let text_padding = egui::vec2(6.0, 4.0);
    let font_id = egui::FontId::proportional(13.0);
    let header_text_color = ui.visuals().strong_text_color();
    let body_text_color = ui.visuals().text_color();

    let bold_font_id = egui::FontId::proportional(13.0);
    let italic_font_id = egui::FontId::proportional(13.0);
    let code_font_id = egui::FontId::monospace(12.0);
    let code_bg = colors::code_bg(ui.visuals());

    let mut row_galleys: Vec<Vec<std::sync::Arc<egui::epaint::text::Galley>>> = Vec::new();
    let mut row_heights: Vec<f32> = Vec::new();

    for (cells, is_header) in &rows {
        let mut galleys = Vec::with_capacity(region.columns);
        let mut row_height: f32 = if *is_header { 26.0 } else { 24.0 };

        for (col_idx, col_width) in column_widths.iter().enumerate() {
            let text = cells.get(col_idx).map(String::as_str).unwrap_or("");
            let base_color = if *is_header {
                header_text_color
            } else {
                body_text_color
            };
            let wrap_width = (*col_width - (text_padding.x * 2.0)).max(8.0);

            let spans = parse_inline_spans(text);
            let galley = build_cell_galley(
                ui,
                &spans,
                base_color,
                wrap_width,
                &font_id,
                &bold_font_id,
                &italic_font_id,
                &code_font_id,
                code_bg,
                *is_header,
            );
            row_height = row_height.max(galley.size().y + (text_padding.y * 2.0));
            galleys.push(galley);
        }

        row_galleys.push(galleys);
        row_heights.push(row_height);
    }

    let total_height: f32 = row_heights.iter().sum();
    let (table_rect, _) = ui.allocate_exact_size(
        egui::vec2(table_width, total_height.max(1.0)),
        egui::Sense::hover(),
    );

    let painter = ui.painter();
    painter.rect_filled(table_rect, 0.0, table_bg);

    let mut y = table_rect.top();
    for (row_idx, ((_cells, is_header), galleys)) in rows.iter().zip(row_galleys.iter()).enumerate()
    {
        let row_height = row_heights[row_idx];
        let row_rect = egui::Rect::from_min_size(
            egui::pos2(table_rect.left(), y),
            egui::vec2(table_rect.width(), row_height),
        );

        painter.rect_filled(row_rect, 0.0, if *is_header { header_bg } else { row_bg });

        let mut x = table_rect.left();
        for (col_idx, col_width) in column_widths.iter().enumerate() {
            let text_color = if *is_header {
                header_text_color
            } else {
                body_text_color
            };
            let galley = &galleys[col_idx];
            let align = region
                .alignments
                .get(col_idx)
                .copied()
                .unwrap_or(TableAlignment::Left);

            let min_x = x + text_padding.x;
            let max_x = (x + col_width - text_padding.x).max(min_x);
            let unclamped_x = match align {
                TableAlignment::Left => min_x,
                TableAlignment::Center => {
                    min_x + ((max_x - min_x - galley.size().x).max(0.0) / 2.0)
                }
                TableAlignment::Right => max_x - galley.size().x,
            };
            let text_x = unclamped_x.clamp(min_x, max_x);

            let text_pos = egui::pos2(text_x, y + text_padding.y);
            painter.galley(text_pos, galley.clone(), text_color);
            x += col_width;
        }

        y += row_height;
    }

    let stroke = Stroke::new(1.0, border);
    painter.rect_stroke(table_rect, egui::Rounding::ZERO, stroke);

    let mut x = table_rect.left();
    for col_width in column_widths
        .iter()
        .take(column_widths.len().saturating_sub(1))
    {
        x += col_width;
        painter.line_segment(
            [
                egui::pos2(x, table_rect.top()),
                egui::pos2(x, table_rect.bottom()),
            ],
            stroke,
        );
    }

    let mut y = table_rect.top();
    for row_height in row_heights.iter().take(row_heights.len().saturating_sub(1)) {
        y += row_height;
        painter.line_segment(
            [
                egui::pos2(table_rect.left(), y),
                egui::pos2(table_rect.right(), y),
            ],
            stroke,
        );
    }
}

fn compute_table_column_widths(columns: usize, total_width: f32, _min_cell_width: f32) -> Vec<f32> {
    if columns == 0 {
        return Vec::new();
    }

    let available = total_width.max(1.0);
    let base = available / columns as f32;
    let mut widths = vec![base; columns];

    let used_without_last: f32 = widths.iter().take(columns - 1).sum();
    widths[columns - 1] = (available - used_without_last).max(0.0);

    widths
}

fn render_blockquote(ui: &mut egui::Ui, spans: &[InlineSpan]) {
    let bar_color = colors::muted(ui.visuals());
    let quote = egui::Frame::none()
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 0.0,
            top: 2.0,
            bottom: 2.0,
        })
        .show(ui, |ui| {
            render_spans(ui, spans, 14.0);
        });

    let rect = quote.response.rect;
    let x = rect.left() + 4.0;
    let y_start = rect.top() + 2.0;
    let y_end = rect.bottom() - 2.0;
    if y_end > y_start {
        ui.painter().line_segment(
            [egui::pos2(x, y_start), egui::pos2(x, y_end)],
            Stroke::new(2.0, bar_color),
        );
    }
}

fn render_spans(ui: &mut egui::Ui, spans: &[InlineSpan], size: f32) {
    ui.horizontal_wrapped(|ui| {
        for span in spans {
            match span {
                InlineSpan::Link { text, url } => {
                    ui.hyperlink_to(text, url);
                }
                InlineSpan::Text {
                    text,
                    bold,
                    italic,
                    underline,
                    strike,
                    code,
                } => {
                    let segments = split_emoji_segments(text);
                    for (segment_text, emoji_color) in segments {
                        if segment_text.is_empty() {
                            continue;
                        }
                        let mut rich = RichText::new(&segment_text).size(size);
                        if *bold {
                            rich = rich.strong();
                        }
                        if *italic {
                            rich = rich.italics();
                        }
                        if *underline {
                            rich = rich.underline();
                        }
                        if *strike {
                            rich = rich.strikethrough();
                        }
                        if *code {
                            rich = rich.monospace();
                        }
                        if let Some(color) = emoji_color {
                            rich = rich.color(color);
                        }
                        ui.label(rich);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_events_to_model_blocks() {
        let content = "# Heading\n- **item**\n> quote with [link](https://example.com)\n\n```rust\nlet x = 1;\n```\n<think>\nponder\n</think>\n| A | B |\n|---|---|\n| 1 | 2 |\n";
        let model = parse_markdown_to_model(content);

        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Heading { level: 1, .. })));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::ListItem { .. })));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::Blockquote(_))));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::CodeBlock { .. })));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::ThinkBlock(_))));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::TableHeader { .. })));
        assert!(model
            .blocks
            .iter()
            .any(|b| matches!(b, MarkdownBlock::TableRow(_))));
    }

    #[test]
    fn handles_incomplete_markdown_code_fence() {
        let content = "```rust\nlet x = 1;";
        let model = parse_markdown_to_model(content);

        assert!(model.blocks.iter().any(|b| {
            matches!(
                b,
                MarkdownBlock::CodeBlock { language: Some(lang), lines }
                    if lang == "rust" && lines == &vec!["let x = 1;".to_string()]
            )
        }));
    }

    #[test]
    fn groups_consecutive_table_blocks_into_region() {
        let blocks = vec![
            MarkdownBlock::Paragraph(vec![]),
            MarkdownBlock::TableHeader {
                cells: vec!["A".to_string(), "B".to_string()],
                alignments: vec![TableAlignment::Left, TableAlignment::Right],
            },
            MarkdownBlock::TableRow(vec!["1".to_string(), "2".to_string()]),
            MarkdownBlock::TableRow(vec!["3".to_string(), "4".to_string()]),
            MarkdownBlock::Paragraph(vec![]),
        ];

        let (region, next_idx) = collect_table_region(&blocks, 1).expect("table region expected");
        assert_eq!(next_idx, 4);
        assert_eq!(region.columns, 2);
        assert_eq!(region.header, Some(vec!["A".to_string(), "B".to_string()]));
        assert_eq!(region.rows.len(), 2);
    }

    #[test]
    fn table_region_supports_row_only_start() {
        let blocks = vec![
            MarkdownBlock::TableRow(vec!["x".to_string()]),
            MarkdownBlock::TableRow(vec!["y".to_string(), "z".to_string()]),
        ];

        let (region, next_idx) =
            collect_table_region(&blocks, 0).expect("row-only table region expected");
        assert_eq!(next_idx, 2);
        assert!(region.header.is_none());
        assert_eq!(region.columns, 2);
        assert_eq!(region.rows.len(), 2);
    }

    #[test]
    fn horizontal_rule_bounds_are_local_and_padded() {
        let bounds = horizontal_rule_x_bounds(200.0, 2.0).expect("bounds expected");
        assert_eq!(bounds, (2.0, 198.0));

        assert!(horizontal_rule_x_bounds(0.0, 2.0).is_none());
        assert!(horizontal_rule_x_bounds(3.0, 2.0).is_none());
    }

    #[test]
    fn table_column_widths_fill_available_width() {
        let widths = compute_table_column_widths(3, 300.0, 72.0);
        assert_eq!(widths.len(), 3);
        let sum: f32 = widths.iter().sum();
        assert!((sum - 300.0).abs() < 0.001);
    }

    #[test]
    fn table_column_widths_never_exceed_available_width() {
        let widths = compute_table_column_widths(6, 180.0, 72.0);
        let sum: f32 = widths.iter().sum();
        assert!(sum <= 180.001);
        assert!(widths.iter().all(|w| *w > 0.0));
    }

    #[test]
    fn parses_table_separator_alignments() {
        let parsed = parse_table_separator_alignments("| :--- | :---: | ---: | --- |").unwrap();
        assert_eq!(
            parsed,
            vec![
                TableAlignment::Left,
                TableAlignment::Center,
                TableAlignment::Right,
                TableAlignment::Left,
            ]
        );
    }

    #[test]
    fn propagates_table_alignments_to_region() {
        let content = "| a | b | c |\n| :--- | :---: | ---: |\n| l | c | r |\n";
        let model = parse_markdown_to_model(content);
        let (region, _) = collect_table_region(&model.blocks, 0).expect("expected table region");
        assert_eq!(
            region.alignments,
            vec![
                TableAlignment::Left,
                TableAlignment::Center,
                TableAlignment::Right,
            ]
        );
    }
}
