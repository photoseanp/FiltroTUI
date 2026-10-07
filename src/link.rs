//! Serial link worker: owns the port in a background thread, talks to the UI via channels.

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

pub enum LinkCmd {
    Open { port: String, baud: u32 },
    Close,
    Send(Vec<u8>),
    ListPorts,
    Quit,
}

pub enum LinkEvent {
    Opened(String),
    Closed,
    Sent(Vec<u8>),
    /// One received line, without the trailing CR.
    Received(Vec<u8>),
    Error(String),
    Ports(Vec<String>),
}

pub struct Link {
    tx: Sender<LinkCmd>,
    pub rx: Receiver<LinkEvent>,
}

impl Link {
    pub fn spawn() -> Link {
        let (ctx, crx) = mpsc::channel();
        let (etx, erx) = mpsc::channel();
        thread::spawn(move || worker(crx, etx));
        Link { tx: ctx, rx: erx }
    }

    pub fn send(&self, cmd: LinkCmd) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.tx.send(LinkCmd::Quit);
    }
}

fn worker(crx: Receiver<LinkCmd>, etx: Sender<LinkEvent>) {
    let mut port: Option<Box<dyn SerialPort>> = None;
    let mut buf: Vec<u8> = Vec::new();

    loop {
        loop {
            match crx.try_recv() {
                Ok(LinkCmd::Quit) => return,
                Ok(LinkCmd::ListPorts) => {
                    let names: Vec<String> = serialport::available_ports()
                        .map(|v| v.into_iter().map(|p| p.port_name).collect())
                        .unwrap_or_default();
                    let _ = etx.send(LinkEvent::Ports(names));
                }
                Ok(LinkCmd::Open { port: name, baud }) => {
                    let res = serialport::new(name.clone(), baud)
                        .data_bits(DataBits::Eight)
                        .parity(Parity::None)
                        .stop_bits(StopBits::One)
                        .flow_control(FlowControl::None)
                        .timeout(Duration::from_millis(50))
                        .open();
                    match res {
                        Ok(p) => {
                            port = Some(p);
                            buf.clear();
                            let _ = etx.send(LinkEvent::Opened(name));
                        }
                        Err(e) => {
                            let _ = etx.send(LinkEvent::Error(format!("open {}: {}", name, e)));
                        }
                    }
                }
                Ok(LinkCmd::Close) => {
                    if port.take().is_some() {
                        let _ = etx.send(LinkEvent::Closed);
                    }
                }
                Ok(LinkCmd::Send(bytes)) => match port.as_mut() {
                    Some(p) => {
                        let r = p.write_all(&bytes);
                        let r = r.and_then(|_| p.flush());
                        match r {
                            Ok(_) => {
                                let _ = etx.send(LinkEvent::Sent(bytes));
                            }
                            Err(e) => {
                                let _ = etx.send(LinkEvent::Error(format!("write: {}", e)));
                            }
                        }
                    }
                    None => {
                        let _ = etx.send(LinkEvent::Error("not connected".into()));
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        let mut failed = false;
        if let Some(p) = port.as_mut() {
            let mut tmp = [0u8; 256];
            match p.read(&mut tmp) {
                Ok(n) if n > 0 => {
                    buf.extend_from_slice(&tmp[..n]);
                    while let Some(pos) = buf.iter().position(|b| *b == 0x0D) {
                        let line: Vec<u8> = buf.drain(..=pos).collect();
                        let _ = etx.send(LinkEvent::Received(line[..line.len() - 1].to_vec()));
                    }
                }
                Ok(_) => {}
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    let _ = etx.send(LinkEvent::Error(format!("read: {}", e)));
                    failed = true;
                }
            }
        } else {
            thread::sleep(Duration::from_millis(20));
        }
        if failed {
            port = None;
            let _ = etx.send(LinkEvent::Closed);
        }
    }
}
