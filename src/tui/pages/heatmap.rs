use super::*;

pub(super) fn render_heatmap(state: &AppState, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "◈ HEATMAP (24H)",
        theme.section_header(),
    )));
    push_divider(&mut lines, theme, width.saturating_sub(2).max(32));

    if state.config.pairs.watchlist.is_empty() {
        lines.push(Line::from(Span::styled(
            "No pairs configured in watchlist.",
            theme.text_muted(),
        )));
        return lines;
    }

    let cell_width = 12usize;
    let cols = (width / cell_width).max(1);
    let mut row_spans: Vec<Span<'static>> = Vec::new();

    for (idx, pair) in state
        .config
        .pairs
        .watchlist
        .iter()
        .take(MAX_RENDER_ITEMS_PER_LIST)
        .enumerate()
    {
        let short = pair.replace("USDT", "");
        let (label, style) = if let Some(t) = state.get_ticker(pair) {
            let pct = t
                .price_change_pct_24h
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0);
            (
                format!(" {:<4} {:+4.1}% ", short, pct),
                theme.heatmap_cell(pct),
            )
        } else {
            (
                format!(" {:<4}   n/a ", short),
                Style::default().fg(theme.text_muted).bg(theme.bg_selected),
            )
        };

        row_spans.push(Span::styled(label, style));
        row_spans.push(Span::styled(" ", theme.text()));

        if (idx + 1) % cols == 0 {
            lines.push(Line::from(row_spans));
            row_spans = Vec::new();
        }
    }

    if !row_spans.is_empty() {
        lines.push(Line::from(row_spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Legend: ", theme.table_header()),
        Span::styled(" +4%+ ", theme.heatmap_cell(5.0)),
        Span::styled(" +1% ", theme.heatmap_cell(2.0)),
        Span::styled(" flat ", theme.heatmap_cell(0.0)),
        Span::styled(" -1% ", theme.heatmap_cell(-2.0)),
        Span::styled(" -4%- ", theme.heatmap_cell(-5.0)),
    ]));

    lines
}
