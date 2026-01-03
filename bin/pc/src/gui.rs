use std::collections::{HashMap, hash_map};
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

use iced::futures::SinkExt;
use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Renderer, Subscription, Task, Theme, stream};
use iced_plot::{Color, LineStyle, MarkerStyle, PlotWidget, PlotWidgetBuilder, Series};
use interfaces::{CommObject, MAX_PACKET_SIZE};
use serial2_tokio::SerialPort;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone)]
pub enum Message {
    ConnectPressed,
    Connected(String),
    UpdatePorts(Vec<PathBuf>),
    ConnectRequest(PathBuf),
    DisconnectRequest(PathBuf),
    SenderReady(PathBuf, Sender<CommObject>),
    PortMessageReceived(PathBuf, CommObject),
    PortMessageSend(PathBuf, CommObject),
    NoOp,
    PlotMessage(iced_plot::PlotUiMessage),
}

pub struct AppConnection {
    tx: Option<Sender<CommObject>>,
    plot: PlotWidget,
    messages: Vec<String>,
}

impl Default for AppConnection {
    fn default() -> Self {
        Self {
            tx: None,
            plot: PlotWidgetBuilder::new()
                .with_autoscale_on_updates(true)
                .with_y_lim(0.0, 4096.0)
                .with_y_label("Value")
                .with_y_tick_labels(true)
                .build()
                .expect("Plot build failed"),
            messages: vec![],
        }
    }
}

pub struct App {
    display_text: String,
    is_loading: bool,
    comm_ports: Vec<PathBuf>,
    connections: HashMap<PathBuf, AppConnection>,
}

impl App {
    fn subscription(&self) -> Subscription<Message> {
        let mut active_subscriptions = vec![track_serial_ports()];

        for path in self.connections.keys() {
            active_subscriptions.push(connect_to_port(path.clone()));
        }

        Subscription::batch(active_subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ConnectPressed => {
                self.is_loading = true;
                self.display_text = "Connecting...".into();
                // Task::perform handles the async execution
                Task::perform(connect(), Message::Connected)
            }
            Message::Connected(result) => {
                self.is_loading = false;
                self.display_text = result;
                Task::none()
            }
            Message::UpdatePorts(items) => {
                if items != self.comm_ports {
                    self.comm_ports = items;
                }
                Task::none()
            }
            Message::ConnectRequest(path_buf) => {
                if !self.connections.contains_key(&path_buf) {
                    self.connections.insert(path_buf, AppConnection::default());
                };
                Task::none()
            }
            Message::DisconnectRequest(path_buf) => {
                if self.connections.contains_key(&path_buf) {
                    self.connections.remove(&path_buf);
                };
                Task::none()
            }
            Message::SenderReady(path_buf, tx) => {
                if let hash_map::Entry::Occupied(mut entry) = self.connections.entry(path_buf) {
                    entry.get_mut().tx = Some(tx);
                }
                Task::none()
            }
            Message::PortMessageReceived(path_buf, comm_object) => {
                println!("{path_buf:?}: {comm_object:?}");
                if let Some(connection) = self.connections.get_mut(&path_buf) {
                    match comm_object {
                        CommObject::Text(m) => connection.messages.push(format!("Text: {m}")),
                        CommObject::Err(m) => connection.messages.push(format!("Error: {m}")),
                        CommObject::Samples(items) => {
                            connection.plot.remove_series("samples");
                            let data: Vec<[f64; 2]> = items
                                .iter()
                                .enumerate()
                                .map(|(i, &val)| [i as f64, val as f64])
                                .collect();
                            let series = Series::line_only(data, LineStyle::Solid)
                                .with_marker_style(MarkerStyle::circle(4.0))
                                .with_color(Color::from_rgb(0.8, 0.2, 0.2))
                                .with_label("samples");
                            connection.plot.add_series(series).unwrap();
                        }
                    }
                }
                Task::none()
            }
            Message::PortMessageSend(path_buf, comm_object) => {
                if let Some(connection) = self.connections.get(&path_buf) {
                    if let Some(tx) = &connection.tx {
                        let tx = tx.clone();
                        return Task::perform(
                            async move {
                                let _ = tx.send(comm_object).await;
                            },
                            |_| Message::NoOp,
                        );
                    }
                }
                Task::none()
            }
            Message::NoOp => Task::none(),
            Message::PlotMessage(_plot_ui_message) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut port_list: Column<'_, Message, Theme, Renderer> =
            column![].spacing(10).align_x(Alignment::Start);

        if self.comm_ports.is_empty() {
            port_list = port_list.push(text("Searching for devices..."));
        } else {
            for port in &self.comm_ports {
                // Create a button for each port so the user can select it
                let port_row = text(port.to_str().unwrap_or("Unknown"));

                port_list = if self.connections.contains_key(port) {
                    port_list.push(row![
                        port_row,
                        button("Disconnect").on_press(Message::DisconnectRequest(port.clone()))
                    ])
                } else {
                    port_list.push(row![
                        port_row,
                        button("Connect").on_press(Message::ConnectRequest(port.clone()))
                    ])
                };
            }
        }

        let mut content = row![
            column![
                text(&self.display_text).size(30),
                button(if self.is_loading {
                    "Waiting..."
                } else {
                    "Connect"
                })
                .on_press(Message::ConnectPressed),
                scrollable(port_list),
            ]
            .spacing(20)
            // align_items is now align_x for Columns
            .align_x(Alignment::Center)
        ];

        for (port, connection) in &self.connections {
            let plot_view = connection.plot.view().map(Message::PlotMessage);

            content = content.push(
                column![
                    text(port.to_str().unwrap_or("Unknown")),
                    button("Send test").on_press(Message::PortMessageSend(
                        port.clone(),
                        CommObject::Text("test".into())
                    )),
                    text(format!("messages:\n{}", connection.messages.join("\n"))),
                    plot_view
                ]
                .padding(40),
            );
        }

        // Center the entire column in the window
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

pub fn track_serial_ports() -> Subscription<Message> {
    // We use Subscription::run to wrap a stream
    Subscription::run(|| {
        stream::channel(
            10,
            |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                loop {
                    // 1. Fetch current ports
                    let ports: Vec<PathBuf> = SerialPort::available_ports().unwrap_or_default();

                    // 2. Send to the UI
                    let _ = output.send(Message::UpdatePorts(ports)).await;

                    // 3. Sleep using Tokio (enabled via iced's tokio feature)
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            },
        )
    })
}

pub fn connect_to_port(path: PathBuf) -> Subscription<Message> {
    Subscription::run_with(path.clone(), |path_ref| {
        let path = path_ref.clone();
        stream::channel(
            10,
            move |mut output: iced::futures::channel::mpsc::Sender<Message>| {
                let path_for_read = path.clone();
                let path_for_setup = path.clone();

                async move {
                    println!("Subscription started");

                    // 1. Open and Split the port
                    let port = match SerialPort::open(&path_for_setup, 921600) {
                        Ok(p) => {
                            p.set_dtr(true).unwrap();
                            p
                        }
                        Err(_) => return, // Handle error/notify UI
                    };
                    let (mut reader, mut writer) = tokio::io::split(port);

                    // 2. Channel for UI -> Background Writer
                    let (tx, mut rx) = tokio::sync::mpsc::channel::<CommObject>(10);

                    // 3. Notify UI that we are ready
                    let _ = output.send(Message::SenderReady(path_for_setup, tx)).await;

                    // --- TASK 1: THE WRITER ---
                    // This task waits for messages from rx (UI) and writes them to the port
                    tokio::spawn(async move {
                        println!("Waiting for message to send");
                        while let Some(message) = rx.recv().await {
                            println!("Start sending message");
                            let mut message_buffer = [0u8; 256];
                            match postcard::to_slice(&message, &mut message_buffer) {
                                Ok(_) => match writer.write_all(&message_buffer).await {
                                    Ok(_) => println!("Send: {message:?}"),
                                    Err(e) => println!("Serial Error: {e}"),
                                },
                                Err(e) => println!("Postcard Error: {e}"),
                            }
                        }
                    });

                    // --- TASK 2: THE READER ---
                    // This loop runs in the current subscription future
                    println!("Waiting for message");
                    let mut buf = [0u8; MAX_PACKET_SIZE];
                    let mut count: usize = 0;
                    loop {
                        match reader.read(&mut buf[count..]).await {
                            Ok(0) => break, // Connection closed
                            Ok(n) => {
                                count += n;
                                if count == MAX_PACKET_SIZE {
                                    count = 0;
                                    let result: Result<CommObject, postcard::Error> =
                                        postcard::from_bytes(&buf);
                                    let data = result.unwrap();
                                    let _ = output
                                        .send(Message::PortMessageReceived(
                                            path_for_read.clone(),
                                            data,
                                        ))
                                        .await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            },
        )
    })
}

async fn connect() -> String {
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    "Success: Message Received!".to_string()
}

pub fn run() -> iced::Result {
    // iced::run is the new simplified entry point in 0.13
    iced::application(|| (App::default(), Task::none()), App::update, App::view)
        .title("Minimal GUI")
        .subscription(App::subscription)
        .run()
}

impl Default for App {
    fn default() -> Self {
        Self {
            display_text: "Click to start".into(),
            is_loading: false,
            comm_ports: vec![],
            connections: HashMap::new(),
        }
    }
}
