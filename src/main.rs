use actix_web::{web, App, HttpServer, HttpResponse,  Error};
use actix_cors::Cors;
use actix_multipart::Multipart;
use futures_util::stream::StreamExt;
use reqwest::multipart::{Form, Part};
//use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Write};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

// Esta función inicia el contenedor y devuelve su ID.
fn start_container() -> Result<String, io::Error> {
    let output = Command::new("docker")
        .arg("run")
        .arg("-d")
        .arg("-p")
        .arg("3000:3000")
        .arg("opendronemap/nodeodm")
        .output()?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Esta función detiene el contenedor dado un ID.
fn stop_container(container_id: &str) -> Result<(), io::Error> {
    let output = Command::new("docker")
        .arg("stop")
        .arg(container_id)
        .output()?;
    println!("Docker stop output: {:?}", output);
    Ok(())
}

// Endpoint para iniciar todo el proceso de reconstrucción
async fn start_reconstruction(mut payload: Multipart) -> Result<HttpResponse, Error> {
    match start_container() {
        Ok(container_id) => {
            let client = reqwest::Client::new();
            
            thread::sleep(Duration::from_secs(5));

            // 1. Initialize a new task
            let init_url = "http://localhost:3000/task/new/init";
            let resp_init = client.post(init_url).send().await.unwrap();
            let data: serde_json::Value = resp_init.json().await.unwrap();
            let token = data["uuid"].as_str().expect("Token not found").to_string();

            let mut image_count = 0;

            // Iterate over each field in the multipart form data
            while let Some(Ok(mut field)) = payload.next().await {
                let mut data = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.unwrap();
                    data.extend_from_slice(&chunk);
                }
                image_count += 1;

                // Create a temporary file to store the image
                let mut file = NamedTempFile::new().unwrap();
                file.write_all(&data).unwrap();
                let file_path = file.into_temp_path();
        
                // Read the file content into a vector of bytes
                let mut file_content = Vec::new();
                fs::File::open(&file_path)
                    .expect("Failed to open file")
                    .read_to_end(&mut file_content)
                    .expect("Failed to read file");

                // Upload the image to the server
                let upload_url = format!("http://localhost:3000/task/new/upload/{}?token={}", token, token);
                let part = Part::bytes(file_content).file_name("image.jpg".to_owned());
                let form = Form::new().part("images", part);
                let resp_upload = client.post(&upload_url).multipart(form).send().await.unwrap();
                println!("Uploaded image {} - Response: {:?}", image_count, resp_upload);
            }

            // 3. Commit the task
            let commit_url = format!("http://localhost:3000/task/new/commit/{}", token);
            let resp_commit = client.post(&commit_url).send().await.unwrap();
            println!("Task commit response: {:?}", resp_commit);

            // 4. Verificar si la tarea ha terminado.
            let mut task_complete = false;

            while !task_complete {
                thread::sleep(Duration::from_secs(10));  // Espera antes de verificar nuevamente.
                let info_url = format!("http://localhost:3000/task/{}/info", token);
                let resp_info = match client.get(&info_url).send().await {
                    Ok(resp) => resp,
                    Err(_err) => {
                        // Manejar el error de conexión cerrada antes de completar el mensaje
                        //println!("Error al enviar la solicitud de información de la tarea: {}", err);
                        continue; // Volver al principio del bucle para intentarlo nuevamente
                    }
                };

                if resp_info.status().is_success() {
                    let task_info: serde_json::Value = resp_info.json().await.unwrap();
                    let status_code = task_info["status"]["code"].as_i64().unwrap_or(0);
                    
                    match status_code {
                        20 => {
                            println!("La tarea sigue en desarrollo.");
                        },
                        40 => {
                            println!("La tarea ha sido completada con éxito.");
                            task_complete = true;
                        },
                    
                        _ => {
                            println!("La tarea ha finalizado con un estado desconocido.");
                            break; // Salir del bucle si el estado no es reconocido
                        }
                    }
                } else {
                    println!("Error al obtener información de la tarea: {}", resp_info.status());
                }
            }

            // 5. Descargar el archivo all.zip
            let download_url = format!("http://localhost:3000/task/{}/download/all.zip", token);
            let response = client.get(&download_url).send().await.unwrap();
            let bytes = response.bytes().await.unwrap();
            fs::write("/home/hechicero/Downloads/all.zip", &bytes).unwrap();

            println!("Archivo all.zip descargado con éxito!");

            // 6. Eliminar la tarea
            let remove_url = "http://localhost:3000/task/remove";
            let remove_body = serde_json::json!({
                "uuid": token
            });
            let resp_remove = client.post(remove_url)
                .json(&remove_body)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_remove.text().await.unwrap());

            // Al final, detener el contenedor
            stop_container(&container_id).unwrap();

            Ok(HttpResponse::Ok().body("Proceso de reconstrucción completado con éxito"))
        },
        Err(_) => Ok(HttpResponse::InternalServerError().body("Error al iniciar el contenedor")),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"]);

            App::new()
            .app_data(web::PayloadConfig::default().limit(1024 * 1024 * 10))
        
            .wrap(cors)
            .service(web::resource("/start_reconstruction").route(web::post().to(start_reconstruction)))
    })
    .bind("127.0.0.1:3001")?
    .run()
    .await
}




/*use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_cors::Cors;
use futures::StreamExt;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Write};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

// Esta función inicia el contenedor y devuelve su ID.
fn start_container() -> Result<String, io::Error> {
    let output = Command::new("docker")
        .arg("run")
        .arg("-d")
        .arg("-p")
        .arg("3000:3000")
        .arg("opendronemap/nodeodm")
        .output()?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Esta función detiene el contenedor dado un ID.
fn stop_container(container_id: &str) -> Result<(), io::Error> {
    let output = Command::new("docker")
        .arg("stop")
        .arg(container_id)
        .output()?;
    println!("Docker stop output: {:?}", output);
    Ok(())
}

// Endpoint para iniciar todo el proceso de reconstrucción
async fn start_reconstruction(mut payload: web::Payload) -> impl Responder {
    match start_container() {
        Ok(container_id) => {
            let client = reqwest::Client::new();
            
            thread::sleep(Duration::from_secs(5));

            // 1. Initialize a new task
            let init_url = "http://localhost:3000/task/new/init";
            let resp_init = client.post(init_url).send().await.unwrap();
            let data: serde_json::Value = resp_init.json().await.unwrap();
            let token = data["uuid"].as_str().expect("Token not found").to_string();

            let mut image_count = 0;

            // Iterate over each image
            while let Some(chunk) = payload.next().await {
                let data = chunk.unwrap();
                image_count += 1;

                // Convertir los bytes a formato hexadecimal y mostrarlos
                //println!("Bytes recibidos para la imagen {}: {:?}", image_count, hex::encode(&data));

                // Create a temporary file to store the image
                let mut file = NamedTempFile::new().unwrap();
                file.write_all(&data).unwrap();

        

                // Read the file content into a vector of bytes
                let mut file_content = Vec::new();
                file.read_to_end(&mut file_content).unwrap();

                // Upload the image to the server
                let upload_url = format!("http://localhost:3000/task/new/upload/{}?token={}", token, token);
                let part = Part::bytes(file_content).file_name("image.jpg".to_owned());
                let form = Form::new().part("images", part);
                let resp_upload = client.post(&upload_url).multipart(form).send().await.unwrap();
                println!("Uploaded image {} - Response: {:?}", image_count, resp_upload);
            }

            // 3. Commit the task
            let commit_url = format!("http://localhost:3000/task/new/commit/{}", token);
            let resp_commit = client.post(&commit_url).send().await.unwrap();
            println!("Task commit response: {:?}", resp_commit);

            // 4. Verificar si la tarea ha terminado.
            let mut task_complete = false;

            while !task_complete {
                thread::sleep(Duration::from_secs(10));  // Espera antes de verificar nuevamente.
                let info_url = format!("http://localhost:3000/task/{}/info", token);
                let resp_info = match client.get(&info_url).send().await {
                    Ok(resp) => resp,
                    Err(err) => {
                        // Manejar el error de conexión cerrada antes de completar el mensaje
                        println!("Error al enviar la solicitud de información de la tarea: {}", err);
                        continue; // Volver al principio del bucle para intentarlo nuevamente
                    }
                };

                if resp_info.status().is_success() {
                    let task_info: serde_json::Value = resp_info.json().await.unwrap();
                    let status_code = task_info["status"]["code"].as_i64().unwrap_or(0);
                    
                    match status_code {
                        20 => {
                            println!("La tarea sigue en desarrollo.");
                        },
                        40 => {
                            println!("La tarea ha sido completada con éxito.");
                            task_complete = true;
                        },
                        _ => {
                            println!("La tarea ha finalizado con un estado desconocido.");
                            break; // Salir del bucle si el estado no es reconocido
                        }
                    }
                } else {
                    println!("Error al obtener información de la tarea: {}", resp_info.status());
                }
            }

            // 5. Descargar el archivo all.zip
            let download_url = format!("http://localhost:3000/task/{}/download/all.zip", token);
            let response = client.get(&download_url).send().await.unwrap();
            let bytes = response.bytes().await.unwrap();
            fs::write("/home/hechicero/Downloads/all.zip", &bytes).unwrap();

            println!("Archivo all.zip descargado con éxito!");

            // 6. Eliminar la tarea
            let remove_url = "http://localhost:3000/task/remove";
            let remove_body = serde_json::json!({
                "uuid": token
            });
            let resp_remove = client.post(remove_url)
                .json(&remove_body)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_remove.text().await.unwrap());

            // Al final, detener el contenedor
            stop_container(&container_id).unwrap();

            HttpResponse::Ok().body("Proceso de reconstrucción completado con éxito")
        },
        Err(_) => HttpResponse::InternalServerError().body("Error al iniciar el contenedor"),
    }
}


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"]);

        App::new()
            .data(web::PayloadConfig::default().limit(1024 * 1024 * 10))
            .wrap(cors)
            .service(web::resource("/start_reconstruction").route(web::post().to(start_reconstruction)))
    })
    .bind("127.0.0.1:3001")?
    .run()
    .await
} */




/*use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_cors::Cors;
use futures::StreamExt;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read};
use std::process::Command;
use std::thread;
use std::time::Duration;

// Esta función inicia el contenedor y devuelve su ID.
fn start_container() -> Result<String, io::Error> {
    let output = Command::new("docker")
        .arg("run")
        .arg("-d")
        .arg("-p")
        .arg("3000:3000")
        .arg("opendronemap/nodeodm")
        .output()?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Esta función detiene el contenedor dado un ID.
fn stop_container(container_id: &str) -> Result<(), io::Error> {
    let output = Command::new("docker")
        .arg("stop")
        .arg(container_id)
        .output()?;
    println!("Docker stop output: {:?}", output);
    Ok(())
}

// Endpoint para iniciar todo el proceso de reconstrucción
async fn start_reconstruction(mut payload: web::Payload) -> impl Responder {
    match start_container() {
        Ok(container_id) => {
            let client = reqwest::Client::new();
            
            thread::sleep(Duration::from_secs(5));

            // 1. Initialize a new task
            let init_url = "http://localhost:3000/task/new/init";
            let resp_init = client.post(init_url).send().await.unwrap();
            let data: serde_json::Value = resp_init.json().await.unwrap();
            let token = data["uuid"].as_str().expect("Token not found").to_string();

            // 2. Upload images
            let upload_url = format!("http://localhost:3000/task/new/upload/{}?token={}", token, token);
            let mut form = Form::new();

            let mut counter = 1;

            let mut image_bytes = Vec::new(); // Vec para almacenar los bytes de la imagen

            while let Some(chunk) = payload.next().await {
                let data = chunk.unwrap();
                println!("Received data chunk: {} bytes", data.len());
                image_bytes.extend_from_slice(&data); // Añadir los bytes del chunk al vector
            }

            let filename = "images.jpg"; // Nombre de archivo único
            let part = Part::stream(image_bytes).file_name(filename.to_string());
            form = form.part("images", part);

            let resp_upload = client.post(&upload_url)
                .multipart(form)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_upload.text().await.unwrap());

            // 3. Commit the task
            let commit_url = format!("http://localhost:3000/task/new/commit/{}", token);
            let resp_commit = client.post(&commit_url).send().await.unwrap();
            println!("{:#?}", resp_commit.text().await.unwrap());

            // 4. Verificar si la tarea ha terminado.
            let mut task_complete = false;

            while !task_complete {
                thread::sleep(Duration::from_secs(10));  // Espera antes de verificar nuevamente.
                let info_url = format!("http://localhost:3000/task/{}/info", token);
                let resp_info = match client.get(&info_url).send().await {
                    Ok(resp) => resp,
                    Err(err) => {
                        // Manejar el error de conexión cerrada antes de completar el mensaje
                        println!("Error al enviar la solicitud de información de la tarea: {}", err);
                        continue; // Volver al principio del bucle para intentarlo nuevamente
                    }
                };

                if resp_info.status().is_success() {
                    let task_info: serde_json::Value = resp_info.json().await.unwrap();
                    let status_code = task_info["status"]["code"].as_i64().unwrap_or(0);
                    
                    match status_code {
                        20 => {
                            println!("La tarea sigue en desarrollo.");
                        },
                        40 => {
                            println!("La tarea ha sido completada con éxito.");
                            task_complete = true;
                        },
                        _ => {
                            println!("La tarea ha finalizado con un estado desconocido.");
                            break; // Salir del bucle si el estado no es reconocido
                        }
                    }
                } else {
                    println!("Error al obtener información de la tarea: {}", resp_info.status());
                }
            }

            // 5. Descargar el archivo all.zip
            let download_url = format!("http://localhost:3000/task/{}/download/all.zip", token);
            let response = client.get(&download_url).send().await.unwrap();
            let bytes = response.bytes().await.unwrap();
            fs::write("/home/hechicero/Downloads/all.zip", &bytes).unwrap();

            println!("Archivo all.zip descargado con éxito!");

            // 6. Eliminar la tarea
            let remove_url = "http://localhost:3000/task/remove";
            let remove_body = serde_json::json!({
                "uuid": token
            });
            let resp_remove = client.post(remove_url)
                .json(&remove_body)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_remove.text().await.unwrap());

            // Al final, detener el contenedor
            stop_container(&container_id).unwrap();

            HttpResponse::Ok().body("Proceso de reconstrucción completado con éxito")
        },
        Err(_) => HttpResponse::InternalServerError().body("Error al iniciar el contenedor"),
    }
}

#[derive(Serialize, Deserialize)]
struct ImageForm {
    // Un campo para el archivo de imagen en formato Bytes
    image: Option<Vec<Vec<u8>>>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"]); // Cambio realizado aquí

        App::new()
            .data(web::PayloadConfig::default().limit(1024 * 1024 * 10))
            .wrap(cors)
            .service(web::resource("/start_reconstruction").route(web::post().to(start_reconstruction)))
    })
    .bind("127.0.0.1:3001")?
    .run()
    .await
} */

/*use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use actix_cors::Cors;
use reqwest::multipart::{Form, Part};
use std::fs::{self, File};
use std::io::Read;
use std::process::Command;
use std::thread;
use std::time::Duration;

// Esta función inicia el contenedor y devuelve su ID.
fn start_container() -> Result<String, std::io::Error> {
    let output = Command::new("docker")
        .arg("run")
        .arg("-d")
        .arg("-p")
        .arg("3000:3000")
        .arg("opendronemap/nodeodm")
        .output()?;
    
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// Esta función detiene el contenedor dado un ID.
fn stop_container(container_id: &str) -> Result<(), std::io::Error> {
    let output = Command::new("docker")
        .arg("stop")
        .arg(container_id)
        .output()?;
    println!("Docker stop output: {:?}", output);
    Ok(())
}

// Endpoint para iniciar todo el proceso de reconstrucción
async fn start_reconstruction() -> impl Responder {
    match start_container() {
        Ok(container_id) => {
            let client = reqwest::Client::new();
            
            thread::sleep(Duration::from_secs(5));

            // 1. Initialize a new task
            let init_url = "http://localhost:3000/task/new/init";
            let resp_init = client.post(init_url).send().await.unwrap();
            let data: serde_json::Value = resp_init.json().await.unwrap();
            let token = data["uuid"].as_str().expect("Token not found").to_string();

            // 2. Upload images
            let upload_url = format!("http://localhost:3000/task/new/upload/{}?token={}", token, token);
            let dir_path = "/media/hechicero/6AA69ABFA69A8AE9/Rust/RustWEBODM/webodm_client/images";
            let mut form = Form::new();

            for entry in fs::read_dir(dir_path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                println!("Checking file: {:?}", path);  
                if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("jpg")) {
                    println!("Processing file: {:?}", path); 
                    
                    let mut file = File::open(&path).unwrap();
                    let mut buffer = Vec::new();
                    file.read_to_end(&mut buffer).unwrap();

                    let filename = path.file_name().unwrap().to_str().unwrap();
                    let part = Part::stream(buffer).mime_str("image/jpeg").unwrap().file_name(filename.to_string());
                    
                    form = form.part("images", part);
                }
            }

            let resp_upload = client.post(&upload_url)
                .multipart(form)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_upload.text().await.unwrap());

            // 3. Commit the task
            let commit_url = format!("http://localhost:3000/task/new/commit/{}", token);
            let resp_commit = client.post(&commit_url).send().await.unwrap();
            println!("{:#?}", resp_commit.text().await.unwrap());

            // 4. Verificar si la tarea ha terminado.
let mut task_complete = false;

while !task_complete {
    thread::sleep(Duration::from_secs(10));  // Espera antes de verificar nuevamente.
    let info_url = format!("http://localhost:3000/task/{}/info", token);
    let resp_info = match client.get(&info_url).send().await {
        Ok(resp) => resp,
        Err(err) => {
            // Manejar el error de conexión cerrada antes de completar el mensaje
            println!("Error al enviar la solicitud de información de la tarea: {}", err);
            continue; // Volver al principio del bucle para intentarlo nuevamente
        }
    };

    if resp_info.status().is_success() {
        let task_info: serde_json::Value = resp_info.json().await.unwrap();
        let status_code = task_info["status"]["code"].as_i64().unwrap_or(0);
        
        match status_code {
            20 => {
                println!("La tarea sigue en desarrollo.");
            },
            40 => {
                println!("La tarea ha sido completada con éxito.");
                task_complete = true;
            },
            _ => {
                println!("La tarea ha finalizado con un estado desconocido.");
                break; // Salir del bucle si el estado no es reconocido
            }
        }
    } else {
        println!("Error al obtener información de la tarea: {}", resp_info.status());
    }
}



            // 5. Descargar el archivo all.zip
            let download_url = format!("http://localhost:3000/task/{}/download/all.zip", token);
            let response = client.get(&download_url).send().await.unwrap();
            let bytes = response.bytes().await.unwrap();
            fs::write("/home/hechicero/Downloads/all.zip", &bytes).unwrap();

            println!("Archivo all.zip descargado con éxito!");

            // 6. Eliminar la tarea
            let remove_url = "http://localhost:3000/task/remove";
            let remove_body = serde_json::json!({
                "uuid": token
            });
            let resp_remove = client.post(remove_url)
                .json(&remove_body)
                .send()
                .await
                .unwrap();
            println!("{:#?}", resp_remove.text().await.unwrap());

            // Al final, detener el contenedor
            stop_container(&container_id).unwrap();

            HttpResponse::Ok().body("Proceso de reconstrucción completado con éxito")
        },
        Err(_) => HttpResponse::InternalServerError().body("Error al iniciar el contenedor"),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST"]); // Cambio realizado aquí


        App::new()
            .wrap(cors)
            .service(web::resource("/start_reconstruction").route(web::post().to(start_reconstruction)))
    })
    .bind("127.0.0.1:3001")?
    .run()
    .await
}
*/
 