//! ESPBrew Cluster Dashboard - Ratatui/Ratzilla implementation
//!
//! Terminal-style dashboard for cluster monitoring and management

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, Wrap},
};

/// Dashboard application state
pub struct Dashboard {
    /// Cluster name
    cluster_name: String,
    /// Selected tab
    selected_tab: usize,
    /// Node data
    nodes: Vec<NodeData>,
    /// Device data
    devices: Vec<DeviceData>,
    /// Job data
    jobs: Vec<JobData>,
    /// Last update timestamp
    last_update: String,
    /// Loading state
    loading: bool,
    /// Error message if any
    error: Option<String>,
}

/// Node information for display
#[derive(Clone, Debug)]
pub struct NodeData {
    pub id: String,
    pub address: String,
    pub role: String,
    pub device_count: usize,
    pub active_jobs: usize,
    pub status: NodeStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeStatus {
    Online,
    Offline,
    Busy,
}

/// Device information for display
#[derive(Clone, Debug)]
pub struct DeviceData {
    pub id: String,
    pub node_id: String,
    pub board_type: String,
    pub status: DeviceStatus,
    pub backend: String,
    pub logical_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceStatus {
    Available,
    Busy { job_id: String },
    Offline,
    Error,
}

/// Job information for display
#[derive(Clone, Debug)]
pub struct JobData {
    pub id: String,
    pub device_id: String,
    pub status: JobStatus,
    pub progress: f32,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

impl Dashboard {
    /// Create new dashboard instance
    pub fn new(cluster_name: String) -> Self {
        Self {
            cluster_name,
            selected_tab: 0,
            nodes: Vec::new(),
            devices: Vec::new(),
            jobs: Vec::new(),
            last_update: "Never".to_string(),
            loading: true,
            error: None,
        }
    }

    /// Update cluster state from API response
    pub fn update_state(&mut self, data: &ClusterState) {
        self.cluster_name = data.cluster_name.clone();
        self.nodes = data.nodes.clone();
        self.devices = data.devices.clone();
        self.jobs = data.jobs.clone();
        self.last_update = chrono::Utc::now().format("%H:%M:%S").to_string();
        self.loading = false;
        self.error = None;
    }

    /// Set error state
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: ratzilla::event::KeyCode) {
        match key {
            ratzilla::event::KeyCode::Left | ratzilla::event::KeyCode::Char('h') => {
                self.selected_tab = self.selected_tab.saturating_sub(1);
            }
            ratzilla::event::KeyCode::Right | ratzilla::event::KeyCode::Char('l') => {
                if self.selected_tab < 3 {
                    self.selected_tab += 1;
                }
            }
            ratzilla::event::KeyCode::Char('1') => self.selected_tab = 0,
            ratzilla::event::KeyCode::Char('2') => self.selected_tab = 1,
            ratzilla::event::KeyCode::Char('3') => self.selected_tab = 2,
            ratzilla::event::KeyCode::Char('4') => self.selected_tab = 3,
            _ => {}
        }
    }

    /// Render the dashboard
    pub fn render(&self, frame: &mut Frame) {
        let size = frame.area();

        // Main layout: header, tabs, content, footer
        let chunks = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Footer
        ])
        .split(size);

        self.render_header(frame, chunks[0]);
        self.render_tabs(frame, chunks[1]);
        self.render_content(frame, chunks[2]);
        self.render_footer(frame, chunks[3]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            Span::styled("🍺 ", Style::default().fg(Color::Yellow)),
            Span::styled(
                "ESPBrew Cluster Dashboard",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" - "),
            Span::styled(&self.cluster_name, Style::default().fg(Color::Cyan)),
        ]);

        let header = Paragraph::new(title).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .border_type(BorderType::Rounded),
        );

        frame.render_widget(header, area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let tabs = vec!["Overview", "Nodes", "Devices", "Jobs"];
        let titles: Vec<Line> = tabs
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                if i == self.selected_tab {
                    Line::from(vec![
                        Span::styled(">", Style::default().fg(Color::Green)),
                        Span::raw(" "),
                        Span::styled(
                            t,
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(t, Style::default().fg(Color::DarkGray)),
                    ])
                }
            })
            .collect();

        let tabs_paragraph = Paragraph::new(titles).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL & !Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(tabs_paragraph, area);
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        if self.loading {
            self.render_loading(frame, area);
        } else if let Some(ref error) = self.error {
            self.render_error(frame, area, error);
        } else {
            match self.selected_tab {
                0 => self.render_overview(frame, area),
                1 => self.render_nodes(frame, area),
                2 => self.render_devices(frame, area),
                3 => self.render_jobs(frame, area),
                _ => self.render_overview(frame, area),
            }
        }
    }

    fn render_loading(&self, frame: &mut Frame, area: Rect) {
        let text = vec![
            Line::from(vec![
                Span::styled("⏳", Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(
                    "Connecting to cluster...",
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("→", Style::default().fg(Color::DarkGray)),
                Span::raw(" Ensure cluster server is running"),
            ]),
            Line::from(vec![
                Span::styled("→", Style::default().fg(Color::DarkGray)),
                Span::raw(" Check /api/v1/cluster/status endpoint"),
            ]),
        ];

        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_error(&self, frame: &mut Frame, area: Rect, error: &str) {
        let text = vec![
            Line::from(vec![
                Span::styled("✖", Style::default().fg(Color::Red)),
                Span::raw(" "),
                Span::styled(
                    "Connection Error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(error, Style::default().fg(Color::Red))]),
        ];

        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_overview(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(10), // Stats grid
            Constraint::Min(0),     // Lists below
        ])
        .split(area);

        // Stats section
        let stats_chunks = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(chunks[0]);

        let node_count = self.nodes.len();
        let device_count = self.devices.len();
        let available: usize = self
            .devices
            .iter()
            .filter(|d| d.status == DeviceStatus::Available)
            .count();
        let active_jobs: usize = self
            .devices
            .iter()
            .filter(|d| matches!(d.status, DeviceStatus::Busy { .. }))
            .count();

        self.render_stat_card(
            frame,
            stats_chunks[0],
            "Nodes",
            &node_count.to_string(),
            Color::Cyan,
        );
        self.render_stat_card(
            frame,
            stats_chunks[1],
            "Devices",
            &device_count.to_string(),
            Color::Green,
        );
        self.render_stat_card(
            frame,
            stats_chunks[2],
            "Available",
            &available.to_string(),
            Color::Blue,
        );
        self.render_stat_card(
            frame,
            stats_chunks[3],
            "Active Jobs",
            &active_jobs.to_string(),
            Color::Yellow,
        );

        // Quick lists section
        let list_chunks =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

        self.render_nodes_list(frame, list_chunks[0]);
        self.render_devices_list(frame, list_chunks[1]);
    }

    fn render_stat_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        label: &str,
        value: &str,
        color: Color,
    ) {
        let content = vec![
            Line::from(vec![Span::styled(
                value,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                label,
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        let card = Paragraph::new(content).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .border_type(BorderType::Rounded),
        );

        frame.render_widget(card, area);
    }

    fn render_nodes(&self, frame: &mut Frame, area: Rect) {
        if self.nodes.is_empty() {
            self.render_empty(frame, area, "No nodes connected", "📡");
            return;
        }

        let rows: Vec<Row> = self
            .nodes
            .iter()
            .map(|n| {
                let status = match n.status {
                    NodeStatus::Online => "● Online",
                    NodeStatus::Offline => "○ Offline",
                    NodeStatus::Busy => "◐ Busy",
                };
                let color = match n.status {
                    NodeStatus::Online => Color::Green,
                    NodeStatus::Offline => Color::DarkGray,
                    NodeStatus::Busy => Color::Yellow,
                };

                Row::new(vec![
                    Cell::from(n.id.as_str()),
                    Cell::from(n.address.as_str()),
                    Cell::from(n.device_count.to_string()),
                    Cell::from(n.active_jobs.to_string()),
                    Cell::from(status).style(Style::default().fg(color)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Percentage(25),
                Constraint::Percentage(30),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
                Constraint::Percentage(15),
            ],
        )
        .header(
            Row::new(vec!["Node", "Address", "Devices", "Jobs", "Status"])
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .title(" Cluster Nodes ")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .widths(&[
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
        ]);

        frame.render_widget(table, area);
    }

    fn render_devices(&self, frame: &mut Frame, area: Rect) {
        if self.devices.is_empty() {
            self.render_empty(frame, area, "No devices detected", "🔌");
            return;
        }

        let rows: Vec<Row> = self
            .devices
            .iter()
            .map(|d| {
                let status = match &d.status {
                    DeviceStatus::Available => "● Available",
                    DeviceStatus::Busy { job_id: _ } => "◐ Busy",
                    DeviceStatus::Offline => "○ Offline",
                    DeviceStatus::Error => "✖ Error",
                };
                let color = match &d.status {
                    DeviceStatus::Available => Color::Blue,
                    DeviceStatus::Busy { .. } => Color::Yellow,
                    DeviceStatus::Offline => Color::DarkGray,
                    DeviceStatus::Error => Color::Red,
                };

                let name = d.logical_name.as_ref().unwrap_or(&d.id);

                Row::new(vec![
                    Cell::from(name.as_str()),
                    Cell::from(d.node_id.as_str()),
                    Cell::from(d.board_type.as_str()),
                    Cell::from(status).style(Style::default().fg(color)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Percentage(30),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(20),
            ],
        )
        .header(
            Row::new(vec!["Device", "Node", "Type", "Status"])
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .title(" Connected Devices ")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(table, area);
    }

    fn render_jobs(&self, frame: &mut Frame, area: Rect) {
        if self.jobs.is_empty() {
            self.render_empty(frame, area, "No active jobs", "⚙️");
            return;
        }

        let rows: Vec<Row> = self
            .jobs
            .iter()
            .map(|j| {
                let status = match j.status {
                    JobStatus::Queued => "○ Queued",
                    JobStatus::Running => "◐ Running",
                    JobStatus::Completed => "● Done",
                    JobStatus::Failed => "✖ Failed",
                };
                let color = match j.status {
                    JobStatus::Queued => Color::DarkGray,
                    JobStatus::Running => Color::Yellow,
                    JobStatus::Completed => Color::Green,
                    JobStatus::Failed => Color::Red,
                };

                let progress = format!("{:.0}%", j.progress * 100.0);

                Row::new(vec![
                    Cell::from(j.id.as_str()),
                    Cell::from(j.device_id.as_str()),
                    Cell::from(progress),
                    Cell::from(status).style(Style::default().fg(color)),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Percentage(30),
                Constraint::Percentage(25),
                Constraint::Percentage(15),
                Constraint::Percentage(30),
            ],
        )
        .header(
            Row::new(vec!["Job ID", "Device", "Progress", "Status"])
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .title(" Active Jobs ")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(table, area);
    }

    fn render_nodes_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<Line> = self
            .nodes
            .iter()
            .map(|n| {
                let status = match n.status {
                    NodeStatus::Online => "●",
                    NodeStatus::Offline => "○",
                    NodeStatus::Busy => "◐",
                };
                Line::from(vec![
                    Span::styled(
                        status,
                        Style::default().fg(match n.status {
                            NodeStatus::Online => Color::Green,
                            NodeStatus::Offline => Color::DarkGray,
                            NodeStatus::Busy => Color::Yellow,
                        }),
                    ),
                    Span::raw(" "),
                    Span::styled(&n.id, Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(
                        format!("({} devices)", n.device_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect();

        let content = if items.is_empty() {
            vec![Line::from(vec![Span::styled(
                "No nodes",
                Style::default().fg(Color::DarkGray),
            )])]
        } else {
            items
        };

        let list = Paragraph::new(content).block(
            Block::default()
                .title(" Nodes ")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(list, area);
    }

    fn render_devices_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<Line> = self
            .devices
            .iter()
            .take(10)
            .map(|d| {
                let status = match &d.status {
                    DeviceStatus::Available => "●",
                    DeviceStatus::Busy { .. } => "◐",
                    DeviceStatus::Offline => "○",
                    DeviceStatus::Error => "✖",
                };
                let name = d.logical_name.as_ref().unwrap_or(&d.id);
                Line::from(vec![
                    Span::styled(
                        status,
                        Style::default().fg(match &d.status {
                            DeviceStatus::Available => Color::Blue,
                            DeviceStatus::Busy { .. } => Color::Yellow,
                            DeviceStatus::Offline => Color::DarkGray,
                            DeviceStatus::Error => Color::Red,
                        }),
                    ),
                    Span::raw(" "),
                    Span::styled(name, Style::default().fg(Color::White)),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}]", d.board_type),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect();

        let content = if items.is_empty() {
            vec![Line::from(vec![Span::styled(
                "No devices",
                Style::default().fg(Color::DarkGray),
            )])]
        } else {
            items
        };

        let list = Paragraph::new(content).block(
            Block::default()
                .title(" Devices ")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(list, area);
    }

    fn render_empty(&self, frame: &mut Frame, area: Rect, message: &str, icon: &str) {
        let text = vec![
            Line::from(vec![Span::styled(
                icon,
                Style::default().fg(Color::DarkGray),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                message,
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        let paragraph = Paragraph::new(text).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let text = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled("scroll", Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled("←→", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled("tabs", Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled("1-4", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled("jump", Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled("Updated: ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.last_update.as_str(), Style::default().fg(Color::Cyan)),
        ]);

        let footer = Paragraph::new(text).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL & !Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(footer, area);
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new("espbrew-cluster".to_string())
    }
}

/// Cluster state from API
#[derive(Clone, Debug)]
pub struct ClusterState {
    pub cluster_name: String,
    pub nodes: Vec<NodeData>,
    pub devices: Vec<DeviceData>,
    pub jobs: Vec<JobData>,
}
