use actix_web::{web, App, HttpServer, HttpResponse, middleware, get, post, delete};
use actix_cors::Cors;
use mongodb::{Client, bson::{doc, oid::ObjectId}};
use mongodb::options::ClientOptions;
use serde::{Deserialize, Serialize};
use serde_json::json;
use chrono::{Utc, DateTime as ChronoDateTime};
use futures::stream::TryStreamExt;

#[derive(Serialize, Deserialize, Clone)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    firstName: String,
    lastName: String,
    email: String,
    password: String,
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profilePic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backgroundImage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    website: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    createdAt: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Message {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    user_id: String,
    from: String,
    subject: String,
    preview: String,
    body: String,
    timestamp: String,
    read: bool,
    starred: bool,
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    isWelcome: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    createdAt: Option<i64>,
}

#[derive(Deserialize)]
struct SignupRequest {
    firstName: String,
    lastName: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    message: String,
    user_id: String,
    firstName: String,
    lastName: String,
    email: String,
    createdAt: String,
    createdAtRelative: String,
}

#[derive(Serialize)]
struct LoginResponse {
    message: String,
    user_id: String,
    firstName: String,
    lastName: String,
    email: String,
    role: Option<String>,
    createdAt: String,
    createdAtRelative: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct SaveMessagesRequest {
    messages: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct MessageUpdateRequest {
    read: Option<bool>,
    starred: Option<bool>,
}

#[derive(Deserialize)]
struct SetRoleRequest {
    user_id: String,
    role: String,
}

#[derive(Deserialize)]
struct ProfilePicRequest {
    profilePic: String,
}

#[derive(Deserialize)]
struct BackgroundRequest {
    backgroundImage: String,
}

#[derive(Deserialize)]
struct ProfileUpdateRequest {
    firstName: Option<String>,
    lastName: Option<String>,
    bio: Option<String>,
    phone: Option<String>,
    location: Option<String>,
    website: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize)]
struct ViewedEmailRequest {
    emailId: String,
}

// Helper function to format Unix timestamp to readable string
fn format_timestamp(timestamp: i64) -> String {
    match ChronoDateTime::<Utc>::from_timestamp_millis(timestamp) {
        Some(datetime) => datetime.format("%b %d, %Y, %H:%M %p").to_string(),
        None => "Unknown".to_string(),
    }
}

// Helper function to format timestamp as relative time
fn format_timestamp_relative(timestamp: i64) -> String {
    let now = Utc::now().timestamp_millis();
    let diff_ms = now - timestamp;
    let diff_seconds = diff_ms / 1000;
    
    if diff_seconds < 60 {
        "just now".to_string()
    } else if diff_seconds < 3600 {
        let mins = diff_seconds / 60;
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if diff_seconds < 86400 {
        let hours = diff_seconds / 3600;
        format!("{} hr{} ago", hours, if hours == 1 { "" } else { "s" })
    } else if diff_seconds < 604800 {
        let days = diff_seconds / 86400;
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else if diff_seconds < 2592000 {
        let weeks = diff_seconds / 604800;
        format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" })
    } else if diff_seconds < 31536000 {
        let months = diff_seconds / 2592000;
        format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
    } else {
        let years = diff_seconds / 31536000;
        format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
    }
}

// USER ENDPOINTS

async fn signup(
    req: web::Json<SignupRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    
    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(_)) => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "Email already exists".to_string(),
            });
        }
        Err(e) => {
            println!("Database error checking email: {}", e);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: "Database error".to_string(),
            });
        }
        Ok(None) => {}
    }
    
    let hashed = bcrypt::hash(&req.password, 4).unwrap_or_else(|_| req.password.clone());
    
    let now_timestamp = Utc::now().timestamp_millis();
    let user = User {
        id: None,
        firstName: req.firstName.clone(),
        lastName: req.lastName.clone(),
        email: req.email.clone(),
        password: hashed,
        role: None,
        profilePic: None,
        backgroundImage: None,
        phone: None,
        location: None,
        website: None,
        bio: None,
        createdAt: Some(now_timestamp),
    };

    match users.insert_one(&user, None).await {
        Ok(result) => {
            let user_id = result.inserted_id.as_object_id()
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| "unknown".to_string());
            
            let created_at_str = format_timestamp(now_timestamp);
            let created_at_relative = format_timestamp_relative(now_timestamp);
            
            println!("User created successfully: {} with ID: {}", req.email, user_id);
            
            HttpResponse::Ok().json(AuthResponse {
                message: "User created".to_string(),
                user_id,
                firstName: req.firstName.clone(),
                lastName: req.lastName.clone(),
                email: req.email.clone(),
                createdAt: created_at_str,
                createdAtRelative: created_at_relative,
            })
        }
        Err(e) => {
            println!("Error inserting user: {}", e);
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Failed to create user".to_string(),
            })
        }
    }
}

async fn login(
    req: web::Json<LoginRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");

    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(user)) => {
            if bcrypt::verify(&req.password, &user.password).unwrap_or(false) {
                let user_id = user.id
                    .map(|oid| oid.to_hex())
                    .unwrap_or_else(|| "unknown".to_string());
                
                let created_at_str = user.createdAt
                    .map(|ts| format_timestamp(ts))
                    .unwrap_or_else(|| "Unknown".to_string());
                
                let created_at_relative = user.createdAt
                    .map(|ts| format_timestamp_relative(ts))
                    .unwrap_or_else(|| "Unknown".to_string());
                
                println!("Login successful for: {}", req.email);
                println!("User role: {:?}", user.role);
                
                HttpResponse::Ok().json(LoginResponse {
                    message: "Login successful".to_string(),
                    user_id,
                    firstName: user.firstName,
                    lastName: user.lastName,
                    email: user.email,
                    role: user.role,
                    createdAt: created_at_str,
                    createdAtRelative: created_at_relative,
                })
            } else {
                println!("Invalid password for: {}", req.email);
                HttpResponse::Unauthorized().json(ErrorResponse {
                    error: "Invalid password".to_string(),
                })
            }
        }
        Ok(None) => {
            println!("User not found: {}", req.email);
            HttpResponse::Unauthorized().json(ErrorResponse {
                error: "User not found".to_string(),
            })
        }
        Err(e) => {
            println!("Database error during login: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "Database error".to_string(),
            })
        }
    }
}

async fn set_role(
    req: web::Json<SetRoleRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");

    let user_oid = match ObjectId::parse_str(&req.user_id) {
        Ok(oid) => oid,
        Err(e) => {
            println!("Invalid user_id format: {} - Error: {}", req.user_id, e);
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
            });
        }
    };

    match users.update_one(
        doc! { "_id": user_oid },
        doc! { "$set": { "role": &req.role } },
        None
    ).await {
        Ok(result) => {
            if result.modified_count > 0 {
                println!("Role set successfully for user: {} to {}", req.user_id, req.role);
                HttpResponse::Ok().json(json!({
                    "message": format!("Role set to {}", req.role)
                }))
            } else {
                println!("User not found for role update: {}", req.user_id);
                HttpResponse::BadRequest().json(ErrorResponse {
                    error: "User not found".to_string(),
                })
            }
        }
        Err(e) => {
            println!("Error setting role: {}", e);
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Failed to set role".to_string(),
            })
        }
    }
}

#[get("/api/user/{user_id}")]
async fn get_user(
    user_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    let user_id_str = user_id.into_inner();
    
    let user_oid = match ObjectId::parse_str(&user_id_str) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})),
    };

    match users.find_one(doc! { "_id": user_oid }, None).await {
        Ok(Some(user)) => {
            let mut response_user = json!({
                "_id": user.id.map(|oid| oid.to_hex()),
                "firstName": user.firstName,
                "lastName": user.lastName,
                "email": user.email,
                "role": user.role,
                "profilePic": user.profilePic,
                "backgroundImage": user.backgroundImage,
                "phone": user.phone,
                "location": user.location,
                "website": user.website,
                "bio": user.bio,
            });
            
            if let Some(created_at_ts) = user.createdAt {
                let created_at_str = format_timestamp(created_at_ts);
                let created_at_relative = format_timestamp_relative(created_at_ts);
                response_user["createdAt"] = json!(created_at_str);
                response_user["createdAtRelative"] = json!(created_at_relative);
            }
            
            HttpResponse::Ok().json(response_user)
        },
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "User not found"})),
        Err(e) => {
            eprintln!("Database error: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Database error"}))
        }
    }
}

#[post("/api/user/{user_id}/profile-pic")]
async fn save_profile_pic(
    user_id: web::Path<String>,
    body: web::Json<ProfilePicRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    let user_id_str = user_id.into_inner();
    
    let user_oid = match ObjectId::parse_str(&user_id_str) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})),
    };

    match users.update_one(
        doc! { "_id": user_oid },
        doc! { "$set": { "profilePic": &body.profilePic } },
        None,
    ).await {
        Ok(_) => {
            println!("Profile pic saved for user: {}", user_id_str);
            HttpResponse::Ok().json(json!({"message": "Profile pic saved"}))
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to save profile pic"}))
        }
    }
}

#[post("/api/user/{user_id}/background")]
async fn save_background(
    user_id: web::Path<String>,
    body: web::Json<BackgroundRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    let user_id_str = user_id.into_inner();
    
    let user_oid = match ObjectId::parse_str(&user_id_str) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})),
    };

    match users.update_one(
        doc! { "_id": user_oid },
        doc! { "$set": { "backgroundImage": &body.backgroundImage } },
        None,
    ).await {
        Ok(_) => {
            println!("Background image saved for user: {}", user_id_str);
            HttpResponse::Ok().json(json!({"message": "Background image saved"}))
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to save background"}))
        }
    }
}

#[post("/api/user/{user_id}/profile")]
async fn update_profile(
    user_id: web::Path<String>,
    body: web::Json<ProfileUpdateRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    let user_id_str = user_id.into_inner();
    
    let user_oid = match ObjectId::parse_str(&user_id_str) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})),
    };

    let mut update_fields = doc! {};
    
    if let Some(ref first_name) = body.firstName {
        update_fields.insert("firstName", first_name);
    }
    if let Some(ref last_name) = body.lastName {
        update_fields.insert("lastName", last_name);
    }
    if let Some(ref bio) = body.bio {
        update_fields.insert("bio", bio);
    }
    if let Some(ref phone) = body.phone {
        update_fields.insert("phone", phone);
    }
    if let Some(ref location) = body.location {
        update_fields.insert("location", location);
    }
    if let Some(ref website) = body.website {
        update_fields.insert("website", website);
    }
    if let Some(ref role) = body.role {
        update_fields.insert("role", role);
    }

    let update_doc = doc! { "$set": update_fields };

    match users.update_one(
        doc! { "_id": user_oid },
        update_doc,
        None,
    ).await {
        Ok(_) => {
            println!("Profile updated for user: {}", user_id_str);
            HttpResponse::Ok().json(json!({"message": "Profile updated"}))
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to update profile"}))
        }
    }
}

// MESSAGE ENDPOINTS

#[post("/api/user/{user_id}/messages")]
async fn save_messages(
    user_id: web::Path<String>,
    body: web::Json<SaveMessagesRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let messages_collection = db.collection::<serde_json::Value>("messages");
    let user_id_str = user_id.into_inner();

    // Clear existing messages for this user
    messages_collection
        .delete_many(doc! { "user_id": &user_id_str }, None)
        .await
        .ok();

    // Insert new messages
    for msg in &body.messages {
        let mut msg_with_user = msg.clone();
        msg_with_user["user_id"] = json!(user_id_str.clone());
        msg_with_user["createdAt"] = json!(Utc::now().timestamp_millis());

        messages_collection.insert_one(msg_with_user, None).await.ok();
    }

    HttpResponse::Ok().json(json!({"message": "Messages saved successfully"}))
}

#[get("/api/user/{user_id}/messages")]
async fn get_messages(
    user_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let messages_collection = db.collection::<serde_json::Value>("messages");
    let user_id_str = user_id.into_inner();

    match messages_collection
        .find(doc! { "user_id": &user_id_str }, None)
        .await
    {
        Ok(mut cursor) => {
            let mut messages = Vec::new();
            while let Ok(Some(msg)) = cursor.try_next().await {
                messages.push(msg);
            }
            HttpResponse::Ok().json(messages)
        }
        Err(e) => {
            eprintln!("Error fetching messages: {}", e);
            HttpResponse::InternalServerError()
                .json(json!({"error": "Failed to fetch messages"}))
        }
    }
}

#[post("/api/user/{user_id}/message/{message_id}")]
async fn update_message(
    path: web::Path<(String, String)>,
    body: web::Json<MessageUpdateRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let messages_collection = db.collection::<serde_json::Value>("messages");
    let (user_id, message_id) = path.into_inner();

    let mut update_fields = doc! {};

    if let Some(read) = body.read {
        update_fields.insert("read", read);
    }
    if let Some(starred) = body.starred {
        update_fields.insert("starred", starred);
    }

    if update_fields.is_empty() {
        return HttpResponse::BadRequest()
            .json(json!({"error": "No fields to update"}));
    }

    let msg_oid = match ObjectId::parse_str(&message_id) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid message ID"})),
    };

    match messages_collection
        .update_one(
            doc! { "user_id": &user_id, "_id": msg_oid },
            doc! { "$set": update_fields },
            None,
        )
        .await
    {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Message updated"})),
        Err(e) => {
            eprintln!("Error updating message: {}", e);
            HttpResponse::InternalServerError()
                .json(json!({"error": "Failed to update message"}))
        }
    }
}

#[delete("/api/user/{user_id}/message/{message_id}")]
async fn delete_message(
    path: web::Path<(String, String)>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let messages_collection = db.collection::<serde_json::Value>("messages");
    let (user_id, message_id) = path.into_inner();

    let msg_oid = match ObjectId::parse_str(&message_id) {
        Ok(oid) => oid,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid message ID"})),
    };

    match messages_collection
        .delete_one(
            doc! { "user_id": &user_id, "_id": msg_oid },
            None,
        )
        .await
    {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Message deleted"})),
        Err(e) => {
            eprintln!("Error deleting message: {}", e);
            HttpResponse::InternalServerError()
                .json(json!({"error": "Failed to delete message"}))
        }
    }
}

// VIEWED EMAILS ENDPOINTS

#[get("/api/user/{user_id}/viewed-emails")]
async fn get_viewed_emails(
    user_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let viewed_collection = db.collection::<serde_json::Value>("viewed_emails");
    let user_id_str = user_id.into_inner();

    match viewed_collection.find_one(doc! { "user_id": &user_id_str }, None).await {
        Ok(Some(doc)) => {
            if let Some(emails) = doc.get("emailIds").and_then(|v| v.as_array()) {
                let email_ids: Vec<String> = emails
                    .iter()
                    .filter_map(|id| id.as_str().map(|s| s.to_string()))
                    .collect();
                
                HttpResponse::Ok().json(json!({"viewedEmails": email_ids}))
            } else {
                HttpResponse::Ok().json(json!({"viewedEmails": []}))
            }
        }
        Ok(None) => HttpResponse::Ok().json(json!({"viewedEmails": []})),
        Err(e) => {
            eprintln!("Error fetching viewed emails: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch viewed emails"}))
        }
    }
}

#[post("/api/user/{user_id}/viewed-emails")]
async fn mark_email_viewed(
    user_id: web::Path<String>,
    body: web::Json<ViewedEmailRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let viewed_collection = db.collection::<serde_json::Value>("viewed_emails");
    let user_id_str = user_id.into_inner();

    match viewed_collection
        .update_one(
            doc! { "user_id": &user_id_str },
            doc! {
                "$addToSet": { "emailIds": &body.emailId }
            },
            None,
        )
        .await
    {
        Ok(result) => {
            // If no document was updated (user doesn't exist yet), insert one
            if result.modified_count == 0 {
                let _ = viewed_collection
                    .insert_one(
                        serde_json::json!({
                            "user_id": &user_id_str,
                            "emailIds": vec![&body.emailId]
                        }),
                        None,
                    )
                    .await;
            }

            println!("Email {} marked as viewed for user: {}", body.emailId, user_id_str);
            HttpResponse::Ok().json(json!({"message": "Email marked as viewed"}))
        }
        Err(e) => {
            eprintln!("Error marking email as viewed: {}", e);
            HttpResponse::InternalServerError()
                .json(json!({"error": "Failed to mark email as viewed"}))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    
    let mongo_url = std::env::var("MONGO_URL")
        .expect("MONGO_URL environment variable not set");
    
    let client_options = ClientOptions::parse(&mongo_url)
        .await
        .expect("Failed to parse MongoDB connection string");
    
    let client = Client::with_options(client_options)
        .expect("Failed to create MongoDB client");
    
    let db = client.database("my_app");
    let users = db.collection::<User>("users");

    match users.create_index(
        mongodb::IndexModel::builder()
            .keys(doc! { "email": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build(),
        None,
    )
    .await {
        Ok(_) => println!("✓ Email unique index created/verified"),
        Err(e) => println!("Warning: Could not create index: {}", e),
    }
    
    let db = web::Data::new(db);

    println!("Starting server on http://0.0.0.0:8000");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);

        App::new()
            .app_data(db.clone())
            .wrap(cors)
            .wrap(middleware::NormalizePath::trim())
            .route("/api/signup", web::post().to(signup))
            .route("/api/login", web::post().to(login))
            .route("/api/set-role", web::post().to(set_role))
            .service(get_user)
            .service(save_profile_pic)
            .service(save_background)
            .service(update_profile)
            // Message endpoints
            .service(save_messages)
            .service(get_messages)
            .service(update_message)
            .service(delete_message)
            // Viewed emails endpoints
            .service(get_viewed_emails)
            .service(mark_email_viewed)
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}