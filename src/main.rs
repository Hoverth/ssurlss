use std::{ 
    collections::HashMap, 
    fmt, 
    fs::{self, File}, 
    io::{ Cursor, Write },
    time::SystemTime 
};
use serde::{ Deserialize, Serialize };
use random_string::generate;
use tiny_http::{ Header, Server, StatusCode, Response, Request };
use urlencoding::decode;

const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    host: String,
    url_path: String,
    link_path: String,
    port: u16,
    entries: HashMap<String, Entry>,
    entries_len: u64,
    allow_new: bool,
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Port: {}\nURL path: {:?}, Links URL path: {:?}\n{} entries, {}llowing new entries", self.port, self.url_path, self.link_path, self.entries_len, if self.allow_new {"A"} else {"Not a"} )
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Entry {
    id: String,
    url: String,
    created: u64,
    delete: Option<u64>,
}

fn main() {  
    // Default Config 
    let mut config: Config = Config { 
        host: String::from("http://localhost:8000"),
        url_path: String::from(""), 
        link_path: String::from("links"),
        port: 8000,
        entries: HashMap::new(), // parse in from toml or something,
        entries_len: 0,
        allow_new: true,
    };
    
    // Read in toml file (if exists)
    println!("Reading config toml file...");
    if let Ok(file_contents) = fs::read_to_string("ssurlss.toml") {
        if let Ok(c) = toml::from_str(&file_contents) {
            config = c;
        } else {
            println!("Failed to read config toml! Proceeding with defaults...");
        }
    } else {
        println!("Config toml file doesn't exist! Will be created with defaults + envvars.");
    }

    println!("Reading environment variables...");
    for (key, value) in std::env::vars() {
        match key.as_str() {
            "HOST" => { config.host = value },
            "URLPATH" => { config.url_path = value },
            "LINKPATH" => { config.link_path = value.trim_matches(['/']).to_string() },
            "PORT" => { config.port = value.parse().unwrap_or(config.port) },
            "ALLOWNEW" => { config.allow_new = true },
            "DISALLOWNEW" => { config.allow_new = false },
            _ => {}
        }
    }

    let entries_len = config.entries.len() as u64;
    if config.entries_len != entries_len {
        config.entries_len = entries_len;
    }
    write_config(&config);

    println!("\nUsing config: \n{config}\n");

    let server = Server::http(format!("0.0.0.0:{}", config.port));
    if let Err(e) = server {
        println!("Failed to create server!\n{e}");
    } else {
        let server = server.unwrap();
        println!("Started server on port {}", config.port);
    
        for request in server.incoming_requests() {
            print!("{}: {:?}, {:?} // ",
                get_now(),
                request.method(),
                request.url(),
            );
            
            let full_url = request.url();
            let prefix = String::from("/") + &config.url_path;
            
            if full_url == prefix || full_url == prefix.clone() + "/" {
                println!("200 Index {full_url:?}");
                let r = html_resp_incl(include_str!("index.html"));
                let _ = request.respond(r);
            } else if full_url.starts_with(&prefix) {
                let url = full_url.strip_prefix(&prefix).unwrap();
                let prefix = config.link_path.clone() + "/";
                
                if url.contains("favicon.ico") {
                    println!("200 Favicon {full_url:?}");
                    file_resp(include_bytes!("../assets/favicon.ico"), request);
                } else if url.starts_with(&prefix) {
                    // hit a shortened url!
                    let key = url.strip_prefix(&prefix).unwrap();
                    if let Some(entry) = config.entries.get(key) {
                        let to = entry.url.clone();
                        if let Some(delete_time) = entry.delete {
                            if delete_time != 0 && delete_time < get_now() {
                                let name = entry.id.clone();
                                config.entries.remove(&name);
                                show_404(request);
                            } else {
                                redir_resp(&to, request);
                            }
                        } else {
                            redir_resp(&to, request);
                        }
                    } else {
                        show_404(request);
                    }
                } else if url.starts_with("add") && config.allow_new {
                    if url.contains('?') { // receiving data to add 
                        let data = url.strip_prefix("add?").unwrap();
                        let entry = process_entry(&config, data);

                        if entry.url.is_empty() {
                            println!("200 Add (data malformed) {full_url:?}");
                            let r = html_resp_incl(include_str!("add.html"));
                            let _ = request.respond(r);
                            continue;
                        }
    
                        let name = entry.id.clone();
                        let _ = config.entries.insert(name.clone(), entry);
                        config.entries_len += 1;
                        println!("200 Add (data) {full_url:?}");
    
                        write_config(&config);
                        let template = include_str!("added.html");
                        let complete_url = if !config.url_path.is_empty() {
                            format!("{}/{}/{}/{}",config.host.clone(), &config.url_path, &config.link_path, &name)
                        } else {
                            format!("{}/{}/{}",config.host.clone(), &config.link_path, &name)
                        };
                        let r = html_resp_incl(&template.replace("{url}", &complete_url));
                        let _ = request.respond(r);
                    } else { // give the normal add page
                        println!("200 Add (page) {full_url:?}");
                        let r = html_resp_incl(include_str!("add.html"));
                        let _ = request.respond(r);
                    }
                } else {
                    show_404(request);
                }
            }
        }
    }
}

fn process_entry(config: &Config, data: &str) -> Entry {
    let mut entry = Entry {
        url: String::new(), 
        id: generate(<u32 as TryInto<usize>>::try_into((config.entries_len + 1).ilog(52)).unwrap() + 1, LETTERS), 
        created: get_now(), 
        delete: None
    };

    let data = data.split("&").map(|x| {
        let y = x.split("=").collect::<Vec<_>>(); 
        if y.len() >= 2 {
            (y[0], y[1]) 
        } else {("", "")}
    }).collect::<Vec<_>>();

    for (key, val) in &data {
        match *key {
            "url" => { if !val.is_empty() {
                if let Ok(url) = decode(val) { entry.url = url.into_owned() }
            }},
            "id" => { 
                let val = val.to_string();
                if !config.entries.contains_key(&val) && !val.is_empty() {
                    entry.id = val;
                }
            },
            "time" => {
                let time = val.to_string();
                if !time.is_empty() {
                    let timestamp = timestamp_from_str(time);
                    if timestamp > entry.created {
                        entry.delete = Some(timestamp);
                    }
                }
            },
            _ => {}
        }
    }

    entry
}

fn timestamp_from_str(string: String) -> u64 { 
    // in an ideal world, the string should just be the timestamp itself.
    let year: u64 = (parse_timeslice(&string, 0, 4) - 1970u64) * 31_557_600u64;
    let month: u64 = parse_timeslice(&string, 5, 7) * 2_629_800u64;
    let day: u64 = parse_timeslice(&string, 8, 10) * 24u64 * 360u64;
    let hour: u64 = parse_timeslice(&string, 11, 13) * 360u64;
    let min: u64 = parse_timeslice(&string, 16, 18) * 60u64;
    year + month + day + min + hour
}

/// only works for png images
fn file_resp(file: &[u8], request: Request) {
    let response = Response::new(
        StatusCode(200),
        Vec::new(),
        file,
        Some(file.len()),
        None
    );
    let _ = request.respond(response);
}

fn redir_resp(to: &str, request: Request) {
    let from = request.url();
    let response = Response::new(
        StatusCode(302), 
        vec![Header::from_bytes(
            &b"Location"[..], to.as_bytes()
        ).unwrap()],
        "redirecting...".as_bytes(), None, None
    );
    println!("302 {from:?} -> {to:?}");
    let _ = request.respond(response);
}

fn html_resp_incl(content: &str) -> Response<Cursor<Vec<u8>>> {
    let mut r = Response::from_string(content);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=UTF-8"[..]) {
        r = r.with_header(header);
    }; 
    r
}

fn parse_timeslice(string: &str, start: usize, end: usize) -> u64 {
    if let Ok(timestamp) = string[start..end].parse() {
        timestamp
    } else { 0u64 }
}

fn get_now() -> u64 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => { n.as_secs() },
        Err(e) => panic!("Err: this program doesn't work on time-travelling systems\n{e}"),
    }
}

fn show_404(request: Request) {
    println!("404 {:?}", request.url());
    let r = html_resp_incl(include_str!("404.html"));
    let _ = request.respond(r);
}

fn write_config(config: &Config) {
    println!("Writing config toml file...");
    if let Ok(mut file) = File::create("ssurlss.toml") {
        if let Err(e) = write!(file, "{}", toml::to_string(&config).unwrap()) {
            println!("Failed to write config!\n{e}");
        };
    } else { println!("Failed to open toml config file!"); }
}

