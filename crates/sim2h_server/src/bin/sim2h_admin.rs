//! `cargo run --bin sim2h_admin -- --help`

use dns_lookup::lookup_host;
use in_stream::*;
use lib3h_crypto_api::CryptoSystem;
use lib3h_protocol::data_types::*;
use lib3h_sodium::SodiumCryptoSystem;
use sim2h::{
    crypto::{Provenance, SignedWireMessage},
    WireMessage,
};
use std::sync::{Arc, Mutex};
use structopt::StructOpt;
use url2::prelude::*;
extern crate serde_derive;

#[derive(StructOpt)]
#[structopt(name = "sim2h_admin")]
struct Opt {
    #[structopt(long)]
    /// sim2h_server url to connect to
    url: String,
}

fn main() {
    ::std::process::exit(match run_app() {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("error: {:?}", err);
            1
        }
    });
}

fn run_app() -> Result<(), String> {
    let args = Opt::from_args();
    let url = match Url2::try_parse(args.url.clone()) {
        Err(e) => Err(format!("unable to parse url:{} got error: {}", args.url, e))?,
        Ok(url) => url,
    };
    //let uri = Lib3hUri(url.into());
    let host = format!("{}", url.host().unwrap());
    let ip = if host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        println!("looking up: {}", host);
        let ips: Vec<std::net::IpAddr> = lookup_host(&host).map_err(|e| format!("{}", e))?;
        println!("resolved to: {}", ips[0]);
        format!("{}", ips[0])
    };
    let url = Url2::parse(format!("{}://{}:{}", url.scheme(), ip, url.port().unwrap()));

    println!("connecting to: {}", url);
    let mut job = Job::new(&url);
    job.send_wire(WireMessage::Status);
    let timeout = std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(500))
        .unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut frame = WsFrame::default();
        match job.connection.read(&mut frame) {
            Ok(_) => {
                if let WsFrame::Binary(b) = frame {
                    let msg: WireMessage = serde_json::from_slice(&b).unwrap();
                    println!("{:?}", msg);
                    break;
                } else {
                    Err(format!("unexpected {:?}", frame))?;
                }
            }
            Err(e) if e.would_block() => (),
            Err(e) => Err(format!("{}", e))?,
        }
        if std::time::Instant::now() >= timeout {
            Err("timeout waiting for status response".to_string())?;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(())
}

thread_local! {
    pub static CRYPTO: Box<dyn CryptoSystem> = Box::new(SodiumCryptoSystem::new());
}
struct Job {
    agent_id: String,
    #[allow(dead_code)]
    pub_key: Arc<Mutex<Box<dyn lib3h_crypto_api::Buffer>>>,
    sec_key: Arc<Mutex<Box<dyn lib3h_crypto_api::Buffer>>>,
    connection: InStreamWss<InStreamTls<InStreamTcp>>,
}

impl Job {
    pub fn new(connect_uri: &Url2) -> Self {
        let (pub_key, sec_key) = CRYPTO.with(|crypto| {
            let mut pub_key = crypto.buf_new_insecure(crypto.sign_public_key_bytes());
            let mut sec_key = crypto.buf_new_secure(crypto.sign_secret_key_bytes());
            crypto.sign_keypair(&mut pub_key, &mut sec_key).unwrap();
            (pub_key, sec_key)
        });
        let enc = hcid::HcidEncoding::with_kind("hcs0").unwrap();
        let agent_id = enc.encode(&*pub_key).unwrap();
        println!("Generated agent id: {}", agent_id);
        let connection = await_in_stream_connect(connect_uri).unwrap();

        let out = Self {
            agent_id,
            pub_key: Arc::new(Mutex::new(pub_key)),
            sec_key: Arc::new(Mutex::new(sec_key)),
            connection,
        };

        //        out.join_space();

        out
    }

    /// sign a message and send it to sim2h
    pub fn send_wire(&mut self, message: WireMessage) {
        let payload: Opaque = message.into();
        let payload_buf: Box<dyn lib3h_crypto_api::Buffer> = Box::new(payload.clone().as_bytes());
        let sig = base64::encode(
            &*CRYPTO
                .with(|crypto| {
                    let mut sig = crypto.buf_new_insecure(crypto.sign_bytes());
                    crypto
                        .sign(&mut sig, &payload_buf, &*self.sec_key.lock().unwrap())
                        .unwrap();
                    sig
                })
                .read_lock(),
        );
        let signed_message = SignedWireMessage {
            provenance: Provenance::new(self.agent_id.clone().into(), sig.into()),
            payload,
        };
        let to_send: Opaque = signed_message.into();
        self.connection.write(to_send.as_bytes().into()).unwrap();
    }
}

fn await_in_stream_connect(
    connect_uri: &Url2,
) -> Result<InStreamWss<InStreamTls<InStreamTcp>>, String> {
    let timeout = std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(10000))
        .unwrap();

    let mut read_frame = WsFrame::default();

    // keep trying to connect
    loop {
        let config = WssConnectConfig::new(TlsConnectConfig::new(TcpConnectConfig::default()));
        let mut connection =
            InStreamWss::connect(connect_uri, config).map_err(|e| format!("{}", e))?;
        connection.write(WsFrame::Ping(b"".to_vec())).unwrap();

        loop {
            let mut err = false;
            match connection.read(&mut read_frame) {
                Ok(_) => return Ok(connection),
                Err(e) if e.would_block() => (),
                Err(_) => {
                    err = true;
                }
            }

            if std::time::Instant::now() >= timeout {
                Err("could not connect within timeout".to_string())?
            }

            if err {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
