use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;

// Import ratzilla and ratatui with proper structure
use ratzilla::ratatui::{
    Frame, Terminal,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use ratzilla::{DomBackend, WebRenderer, event::KeyCode};

// Import gloo-timers for sleep functionality
use gloo_timers::future::sleep;

/// Dashboard application state
struct Dashboard {
    cluster_name: RefCell<String>,
    selected_tab: RefCell<usize>,
    nodes: RefCell<Vec<NodeInfo>>,
    devices: RefCell<Vec<DeviceInfo>>,
    last_update: RefCell<String>,
    loading: RefCell<bool>,
    error: RefCell<Option<String>>,
}

#[derive(Clone, Debug)]
struct NodeInfo {
    id: String,
    address: String,
    device_count: usize,
    active_jobs: usize,
    status: NodeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NodeStatus {
    Online,
    Offline,
    Busy,
}

#[derive(Clone, Debug)]
struct DeviceInfo {
    id: String,
    node_id: String,
    board_type: String,
    status: DeviceStatus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeviceStatus {
    Available,
    Busy,
    Offline,
}

impl Dashboard {
    fn new() -> Self {
        Self {
            cluster_name: RefCell::new("ESPBrew".to_string()),
            selected_tab: RefCell::new(0),
            nodes: RefCell::new(Vec::new()),
            devices: RefCell::new(Vec::new()),
            last_update: RefCell::new("Connecting...".to_string()),
            loading: RefCell::new(true),
            error: RefCell::new(None),
        }
    }

    fn render(&self, frame: &mut Frame) {
        let size = frame.area();

        // Main layout: header, content, footer
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(size);

        self.render_header(frame, chunks[0]);
        self.render_content(frame, chunks[1]);
        self.render_footer(frame, chunks[2]);
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
        ]);

        let header = Paragraph::new(title).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .border_type(BorderType::Rounded),
        );

        frame.render_widget(header, area);
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        if *self.loading.borrow() {
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
                    Span::raw(" Start cluster: espbrew cluster start"),
                ]),
                Line::from(vec![
                    Span::styled("→", Style::default().fg(Color::DarkGray)),
                    Span::raw(" Then open: http://127.0.0.1:8081/"),
                ]),
            ];

            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            frame.render_widget(paragraph, area);
        } else if let Some(ref error) = *self.error.borrow() {
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
        } else {
            self.render_overview(frame, area);
        }
    }

    fn render_overview(&self, frame: &mut Frame, area: Rect) {
        let stats_chunks = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

        let node_count = self.nodes.borrow().len();
        let device_count = self.devices.borrow().len();

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
        self.render_stat_card(frame, stats_chunks[2], "Available", "0", Color::Blue);
        self.render_stat_card(frame, stats_chunks[3], "Active Jobs", "0", Color::Yellow);
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

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let last_update = self.last_update.borrow();
        let text = Line::from(vec![
            Span::styled("←→", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled("tabs", Style::default().fg(Color::DarkGray)),
            Span::raw(" | "),
            Span::styled("Updated: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&**last_update, Style::default().fg(Color::Cyan)),
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
        Self::new()
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();

    let dashboard = Rc::new(Dashboard::new());

    let backend = DomBackend::new().unwrap();
    let terminal = Terminal::new(backend).unwrap();

    // Set up keyboard handler
    let dashboard_clone = dashboard.clone();
    terminal.on_key_event(move |key_event| match key_event.code {
        KeyCode::Left | KeyCode::Char('h') => {}
        KeyCode::Right | KeyCode::Char('l') => {}
        KeyCode::Char('1') => {}
        KeyCode::Char('2') => {}
        KeyCode::Char('3') => {}
        KeyCode::Char('4') => {}
        _ => {}
    });

    // Set up render loop
    let dashboard_clone = dashboard.clone();
    terminal.draw_web(move |frame| {
        dashboard_clone.render(frame);
    });

    // Start polling for cluster data
    let dashboard_for_poll = dashboard.clone();
    wasm_bindgen_futures::spawn_local(async move {
        poll_cluster_status(dashboard_for_poll).await;
    });
}

async fn poll_cluster_status(dashboard: Rc<Dashboard>) {
    let mut retry_count = 0u32;
    const MAX_RETRIES: u32 = 5;
    const RETRY_DELAY_MS: u32 = 2000;

    loop {
        // Try different URLs in order
        let urls = vec![
            "/api/v1/cluster/status",
            "http://127.0.0.1:8081/api/v1/cluster/status",
            "http://localhost:8081/api/v1/cluster/status",
        ];

        let mut success = false;

        for url in urls {
            let opts = web_sys::RequestInit::new();
            opts.set_method("GET");
            opts.set_mode(web_sys::RequestMode::Cors);

            match web_sys::Request::new_with_str_and_init(url, &opts) {
                Ok(req) => {
                    if let Some(window) = web_sys::window() {
                        let promise = window.fetch_with_request(&req);
                        let result = wasm_bindgen_futures::JsFuture::from(promise).await;

                        if let Ok(response_value) = result {
                            if let Ok(response) = response_value.dyn_into::<web_sys::Response>() {
                                if response.ok() {
                                    if let Ok(json_promise) = response.json() {
                                        if let Ok(json_value) =
                                            wasm_bindgen_futures::JsFuture::from(json_promise).await
                                        {
                                            // Extract cluster status fields
                                            let cluster_name = js_sys::Reflect::get(
                                                &json_value,
                                                &wasm_bindgen::JsValue::from_str("cluster_name"),
                                            );
                                            let device_count = js_sys::Reflect::get(
                                                &json_value,
                                                &wasm_bindgen::JsValue::from_str("device_count"),
                                            );
                                            let node_count = js_sys::Reflect::get(
                                                &json_value,
                                                &wasm_bindgen::JsValue::from_str("node_count"),
                                            );
                                            let available_devices = js_sys::Reflect::get(
                                                &json_value,
                                                &wasm_bindgen::JsValue::from_str(
                                                    "available_devices",
                                                ),
                                            );

                                            if let Some(_name) =
                                                cluster_name.ok().and_then(|v| v.as_string())
                                            {
                                                *dashboard.loading.borrow_mut() = false;
                                                *dashboard.error.borrow_mut() = None;
                                                *dashboard.last_update.borrow_mut() =
                                                    js_sys::Date::new_0()
                                                        .to_locale_time_string("en-US")
                                                        .into();

                                                // Update device and node counts
                                                if let Ok(count_val) = device_count {
                                                    if let Some(count_str) = count_val.as_string() {
                                                        // Parse device count and create mock device entries
                                                        if let Ok(count) =
                                                            count_str.parse::<usize>()
                                                        {
                                                            let mut devices =
                                                                dashboard.devices.borrow_mut();
                                                            devices.clear();
                                                            for i in 0..count {
                                                                devices.push(DeviceInfo {
                                                                    id: format!("device-{}", i),
                                                                    node_id: "master".to_string(),
                                                                    board_type: "ESP32".to_string(),
                                                                    status: DeviceStatus::Available,
                                                                });
                                                            }
                                                        }
                                                    } else if let Some(count_num) =
                                                        count_val.as_f64()
                                                    {
                                                        // Device count is a number
                                                        let mut devices =
                                                            dashboard.devices.borrow_mut();
                                                        devices.clear();
                                                        for i in 0..(count_num as usize) {
                                                            devices.push(DeviceInfo {
                                                                id: format!("device-{}", i),
                                                                node_id: "master".to_string(),
                                                                board_type: "ESP32".to_string(),
                                                                status: DeviceStatus::Available,
                                                            });
                                                        }
                                                    }
                                                }

                                                // Update node count
                                                if let Ok(count_val) = node_count {
                                                    if let Some(count_num) = count_val.as_f64() {
                                                        let mut nodes =
                                                            dashboard.nodes.borrow_mut();
                                                        nodes.clear();
                                                        for i in 0..(count_num as usize) {
                                                            nodes.push(NodeInfo {
                                                                id: format!("node-{}", i),
                                                                address: "127.0.0.1:8081"
                                                                    .to_string(),
                                                                device_count: 4,
                                                                active_jobs: 0,
                                                                status: NodeStatus::Online,
                                                            });
                                                        }
                                                    }
                                                }

                                                success = true;
                                                retry_count = 0;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {}
            }

            if success {
                break;
            }
        }

        if !success {
            retry_count += 1;
            if retry_count >= MAX_RETRIES {
                *dashboard.error.borrow_mut() = Some(
                    "Could not connect to cluster API. Is the cluster server running?".to_string(),
                );
                *dashboard.loading.borrow_mut() = false;
            }
        }

        // Wait before next poll
        sleep(std::time::Duration::from_millis(RETRY_DELAY_MS as u64)).await;
    }
}
