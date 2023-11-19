use std::{collections::VecDeque, rc::Rc};

use clap::Parser;
mod args;
mod icmp;
mod logger;
mod non_async;
mod tcp;
mod udp;
use anyhow::Result;
use eframe::egui;
use egui::{Pos2, RawInput, Ui};
use egui_plot::{Line, Plot};
use tokio::task;

struct PingPlotter {
    data: VecDeque<f64>,
    data_channel: std::sync::mpsc::Receiver<f64>,
}

impl PingPlotter {
    fn new(
        cc: &eframe::CreationContext<'_>,
        data_channel: std::sync::mpsc::Receiver<f64>,
    ) -> Self {
        Self {
            data_channel,
            data: VecDeque::from(vec![0.0; 1000]),
        }
    }
}

impl eframe::App for PingPlotter {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Ping Plotter");
            self.data_channel.try_recv().map_or_else(
                |_| {
                    ctx.request_repaint();
                },
                |data| {
                    self.data.pop_front();
                    self.data.push_back(data);
                    ctx.request_repaint();
                },
            );

            let data = self.data.clone().into_iter().collect::<Vec<_>>();
            let rtt = egui_plot::Line::new(egui_plot::PlotPoints::from_ys_f64(
                data.as_slice(),
            ));
            let plot = Plot::new("Ping RTT")
                .x_axis_label(format!("Timestep [{}]", 1))
                .y_axis_label("RTT [ms]");

            let bounds =
                egui_plot::PlotBounds::from_min_max([0.0, 0.0], [1000.0, 1.0]);

            plot.show(ui, |ui| {
                ui.set_plot_bounds(bounds);
                ui.line(rtt)
            });
        });
    }
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (data_ch_tx, data_ch_rx) = std::sync::mpsc::channel::<f64>();
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    std::thread::spawn(move || {
        rt.block_on(async {
            let args = args::Opts::parse();
            match args.mode {
                args::Modes::Client { proto } => {
                    match proto {
                        args::Protocol::Tcp(opts) => {
                            let mut client = tcp::TCPClient::new(opts)?;
                            client.run(Some(data_ch_tx)).await?;
                        }
                        args::Protocol::Udp(opts) => {
                            let mut client = udp::UDPClient::new(opts)?;
                            client.run().await?;
                        }
                        args::Protocol::Icmp(opts) => {
                            let mut client = icmp::ICMPClient::new(opts)?;
                            client.run().await?;
                        }
                    }
                }
                args::Modes::Server { proto } => {
                    match proto {
                        args::Protocol::Tcp(opts) => {
                            let mut server = tcp::TCPServer::new(opts)?;
                            server.run().await?;
                        }
                        args::Protocol::Udp(opts) => {
                            let mut server = udp::UDPServer::new(opts)?;
                            server.run().await?;
                        }
                        args::Protocol::Icmp(..) => {
                            anyhow::bail!("ICMP server not implemented");
                        }
                    }
                }
            }
            Ok(())
        })
    });
    eframe::run_native(
        "Niceperf Ping Plotter",
        options,
        Box::new(|cc| Box::new(PingPlotter::new(cc, data_ch_rx))),
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

#[cfg(test)]
mod test {
    use common::{delete_venv, init_venv, run_pytest};

    // These functions are for running pytests based on the dummynet network
    // namespace testing environment
    #[test]
    fn integration_test() {
        init_venv();
        run_pytest();
        delete_venv();
    }
}
