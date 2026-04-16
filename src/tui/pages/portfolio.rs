use super::*;

pub(super) fn render_portfolio_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
) {
    let shell = Block::default()
        .title(Span::styled(" Portfolio ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(7),
        ])
        .split(shell_inner);

    let portfolio = &state.portfolio;
    let metrics = portfolio.calculate_metrics();
    let total_value = portfolio.total_value();
    let invested = portfolio.total_position_value();
    let daily_pnl = portfolio.daily_realized_pnl + portfolio.total_unrealized_pnl();
    let daily_pnl_pct = if total_value > Decimal::ZERO {
        (daily_pnl / total_value) * Decimal::from(100)
    } else {
        Decimal::ZERO
    };

    let summary_header = Row::new(vec![
        Cell::from("Total Value"),
        Cell::from("Cash"),
        Cell::from("Invested"),
        Cell::from("PnL Today"),
        Cell::from("Drawdown"),
    ])
    .style(theme.table_header());

    let summary_row = Row::new(vec![
        Cell::from(format_price(total_value)).style(theme.text_bold()),
        Cell::from(format_price(portfolio.cash)).style(theme.text()),
        Cell::from(format_price(invested)).style(theme.text_accent()),
        Cell::from(format!("{} {:+.1}%", format_pnl(daily_pnl), daily_pnl_pct))
            .style(theme.pnl(daily_pnl)),
        Cell::from(format!("-{:.1}%", portfolio.current_drawdown_pct))
            .style(Style::default().fg(theme.loss)),
    ]);

    let summary_table = Table::new(
        vec![summary_row],
        [
            Constraint::Length(13),
            Constraint::Length(13),
            Constraint::Length(13),
            Constraint::Length(18),
            Constraint::Length(10),
        ],
    )
    .header(summary_header)
    .block(
        Block::default()
            .title(Span::styled(" Account Snapshot ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(summary_table, chunks[0]);

    let positions_total = portfolio.positions.len();
    if positions_total == 0 {
        let empty = Paragraph::new(vec![
            Line::from(Span::styled("No open positions.", theme.text_muted())),
            Line::from(Span::styled(
                "Run /signals or wait for scheduler analysis.",
                theme.text_secondary(),
            )),
        ])
        .style(theme.text())
        .block(
            Block::default()
                .title(Span::styled(" Open Positions (0) ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        );
        f.render_widget(empty, chunks[1]);
    } else {
        let positions_header = Row::new(vec![
            Cell::from("Pair"),
            Cell::from("Side"),
            Cell::from("Entry"),
            Cell::from("Now"),
            Cell::from("PnL"),
            Cell::from("SL"),
            Cell::from("TP"),
            Cell::from("Flow"),
        ])
        .style(theme.table_header());

        let viewport_rows = chunks[1].height.saturating_sub(3) as usize;
        let start = scroll.min(positions_total.saturating_sub(1));
        let end = (start + viewport_rows.max(1)).min(positions_total);

        let rows: Vec<Row> = portfolio.positions[start..end]
            .iter()
            .map(|pos| {
                let side_style = if pos.side == PositionSide::Long {
                    Style::default()
                        .fg(theme.signal_long)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(theme.signal_short)
                        .add_modifier(Modifier::BOLD)
                };

                let pair_short = pos.pair.replace("USDT", "");
                Row::new(vec![
                    Cell::from(truncate_ellipsis(&pair_short, 6)).style(theme.text_accent_bold()),
                    Cell::from(
                        if pos.side == PositionSide::Long {
                            "LONG"
                        } else {
                            "SHORT"
                        }
                        .to_string(),
                    )
                    .style(side_style),
                    Cell::from(format_price(pos.entry_price)).style(theme.text()),
                    Cell::from(format_price(pos.current_price)).style(theme.text()),
                    Cell::from(format!(
                        "{} {:+.1}%",
                        format_pnl(pos.unrealized_pnl),
                        pos.unrealized_pnl_pct
                    ))
                    .style(theme.pnl(pos.unrealized_pnl)),
                    Cell::from(format_price(pos.stop_loss)).style(Style::default().fg(theme.loss)),
                    Cell::from(format_price(pos.take_profit))
                        .style(Style::default().fg(theme.profit)),
                    Cell::from(sparkline_from_position(pos.unrealized_pnl_pct))
                        .style(theme.pnl(pos.unrealized_pnl)),
                ])
            })
            .collect();

        let positions_table = Table::new(
            rows,
            [
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(positions_header)
        .block(
            Block::default()
                .title(Span::styled(
                    format!(" Open Positions ({}) ", positions_total),
                    theme.panel_title(),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        )
        .style(theme.text());

        f.render_widget(positions_table, chunks[1]);

        if positions_total > viewport_rows.max(1) {
            let mut scroll_state =
                ScrollbarState::new(positions_total).position(start.min(positions_total - 1));
            f.render_stateful_widget(
                Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
                chunks[1],
                &mut scroll_state,
            );
        }
    }

    let best_trade = portfolio
        .trade_history
        .iter()
        .max_by_key(|t| t.realized_pnl)
        .map(|t| {
            format!(
                "{} {}",
                format_pnl(t.realized_pnl),
                t.pair.replace("USDT", "")
            )
        })
        .unwrap_or_else(|| "n/a".to_string());
    let worst_trade = portfolio
        .trade_history
        .iter()
        .min_by_key(|t| t.realized_pnl)
        .map(|t| {
            format!(
                "{} {}",
                format_pnl(t.realized_pnl),
                t.pair.replace("USDT", "")
            )
        })
        .unwrap_or_else(|| "n/a".to_string());

    let performance_rows = vec![
        Row::new(vec![
            Cell::from("Trades").style(theme.table_header()),
            Cell::from(metrics.total_trades.to_string()).style(theme.text()),
            Cell::from("Win Rate").style(theme.table_header()),
            Cell::from(format!("{:.1}%", metrics.win_rate))
                .style(theme.price_change(metrics.win_rate >= Decimal::from(50))),
        ]),
        Row::new(vec![
            Cell::from("Profit Factor").style(theme.table_header()),
            Cell::from(format!("{:.2}x", metrics.profit_factor)).style(theme.profit_style()),
            Cell::from("Net").style(theme.table_header()),
            Cell::from(format_pnl(metrics.net_profit)).style(theme.pnl(metrics.net_profit)),
        ]),
        Row::new(vec![
            Cell::from("Best").style(theme.table_header()),
            Cell::from(truncate_ellipsis(&best_trade, 22)).style(theme.profit_style()),
            Cell::from("Worst").style(theme.table_header()),
            Cell::from(truncate_ellipsis(&worst_trade, 22)).style(theme.loss_style()),
        ]),
    ];

    let performance_table = Table::new(
        performance_rows,
        [
            Constraint::Length(14),
            Constraint::Length(22),
            Constraint::Length(12),
            Constraint::Min(12),
        ],
    )
    .block(
        Block::default()
            .title(Span::styled(" Performance ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(performance_table, chunks[2]);
}
