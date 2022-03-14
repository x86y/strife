use discord_markdown::parser;
use discord_markdown::parser::Expression;
use std::borrow::Cow;
use std::ops::Range;
use textwrap::Options as WrapOptions;
use tui::style::{Modifier, Style};
use tui::text::Span as UiSpan;
use tui::text::Spans;
use tui::widgets::ListItem;

#[derive(Debug)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
    pub modifier: Modifier,
}

impl Span {
    pub fn range(&self) -> Range<usize> {
        self.offset..self.offset.saturating_add(self.len)
    }
}

fn iterate_expressions(
    plain: &mut String,
    spans: &mut Vec<Span>,
    modifier: &mut Modifier,
    offset: &mut usize,
    expressions: Vec<Expression>,
) {
    for expression in expressions.into_iter() {
        match expression {
            Expression::Text(text) => {
                let len = text.len();
                let span = Span {
                    offset: *offset,
                    len: text.len(),
                    modifier: *modifier,
                };

                plain.push_str(text);
                spans.push(span);
                *offset = offset.saturating_add(len);
            }
            Expression::Bold(expressions) => {
                modifier.insert(Modifier::BOLD);

                iterate_expressions(plain, spans, modifier, offset, expressions);

                modifier.remove(Modifier::BOLD);
            }
            Expression::Italics(expressions) => {
                modifier.insert(Modifier::ITALIC);

                iterate_expressions(plain, spans, modifier, offset, expressions);

                modifier.remove(Modifier::ITALIC);
            }
            _ => {}
        }
    }
}

pub fn render_message<'a, 'b>(
    content: &'a str,
    wrap_options: &'b WrapOptions,
) -> Vec<ListItem<'static>> {
    let mut plain = String::new();
    let mut spans = Vec::new();
    let mut modifier = Modifier::empty();
    let mut offset: usize = 0;

    let expressions = parser::parse(content);

    iterate_expressions(
        &mut plain,
        &mut spans,
        &mut modifier,
        &mut offset,
        expressions,
    );

    let mut lines = vec![];
    let mut offset: usize = 0;
    let mut wrapped = textwrap::wrap(&plain, wrap_options);

    if let Some(mut last) = wrapped.last_mut() {
        let trailing_whitespace = &plain[plain.trim_end_matches(' ').len()..];

        *last = Cow::Owned(format!("{last}{trailing_whitespace}"));
    }

    tracing::debug!("plain = {plain:?}");
    tracing::debug!("spans = {spans:?}");
    tracing::debug!("wrapped = {wrapped:?}");

    // word wrap pass
    for line in wrapped {
        let len = line.len();
        let start = offset;
        let end = offset.saturating_add(len);

        let mut line = vec![];

        // markdown spans on this line
        let markdown_spans = spans.iter().filter(|span| {
            let range = span.range();

            range.start >= start && range.end <= end
        });

        for span in markdown_spans {
            let text = unsafe { get_unchecked(&plain, span.range()) };
            let style = Style::default().add_modifier(span.modifier);

            line.push(UiSpan::styled(text, style));
        }

        lines.push(ListItem::new(Spans::from(line)));

        offset = end;
    }

    lines
}

unsafe fn get_unchecked(string: &str, range: Range<usize>) -> String {
    string.get_unchecked(range).into()
}
