use actix_web::{web, App, HttpServer, HttpResponse, HttpRequest, middleware, get, post, delete};
use actix_cors::Cors;
use mongodb::{Client, bson::{doc, oid::ObjectId}};
use mongodb::options::ClientOptions;
use serde::{Deserialize, Serialize};
use serde_json::json;
use chrono::{Utc, DateTime as ChronoDateTime};
use futures::stream::TryStreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type WsConnections = Arc<RwLock<HashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>>;

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
    coverImage: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen: Option<i64>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChatReplyRef {
    message_id: String,
    content: String,
    sender_id: String,
    sender_name: String,
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
struct CoverImageRequest {
    coverImage: String,
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

#[derive(Deserialize)]
struct SendEmailRequest {
    subject: String,
    body: String,
    userIds: Vec<String>,
    from: String,
}

// ── NEW: HTTP fallback for sending chat messages when WS is down ──
#[derive(Deserialize)]
struct SendChatMessageRequest {
    to: String,
    content: String,
    sender_id: String,
    reply_to: Option<serde_json::Value>,
}

// ── TIMESTAMP HELPERS ────────────────────────────────────────────────────────

fn format_timestamp(timestamp: i64) -> String {
    match ChronoDateTime::<Utc>::from_timestamp_millis(timestamp) {
        Some(datetime) => datetime.format("%b %d, %Y, %H:%M %p").to_string(),
        None => "Unknown".to_string(),
    }
}

fn format_timestamp_relative(timestamp: i64) -> String {
    let now = Utc::now().timestamp_millis();
    let diff_seconds = (now - timestamp) / 1000;
    if diff_seconds < 60        { return "just now".to_string(); }
    let mins = diff_seconds / 60;
    if mins < 60                { return format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" }); }
    let hours = mins / 60;
    if hours < 24               { return format!("{} hr{} ago",  hours, if hours == 1 { "" } else { "s" }); }
    let days = hours / 24;
    if days < 7                 { return format!("{} day{} ago", days,  if days == 1  { "" } else { "s" }); }
    let weeks = days / 7;
    if weeks < 4                { return format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" }); }
    let months = days / 30;
    if months < 12              { return format!("{} month{} ago", months, if months == 1 { "" } else { "s" }); }
    let years = days / 365;
    format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT — WebSocket helpers
// ═══════════════════════════════════════════════════════════════════════════

async fn send_online_status(
    user_id: &str,
    online: bool,
    last_seen: Option<i64>,
    db: &mongodb::Database,
    conns: &WsConnections,
) {
    let convs = db.collection::<serde_json::Value>("conversations");
    let mut cursor = match convs.find(doc! { "participants": user_id }, None).await {
        Ok(c) => c,
        Err(_) => return,
    };
    let payload = serde_json::to_string(&json!({
        "type": "online_status",
        "user_id": user_id,
        "online": online,
        "last_seen": last_seen,
    })).unwrap_or_default();

    let guard = conns.read().await;
    while let Ok(Some(conv)) = cursor.try_next().await {
        if let Some(parts) = conv.get("participants").and_then(|v| v.as_array()) {
            for p in parts {
                if let Some(pid) = p.as_str() {
                    if pid != user_id {
                        if let Some(tx) = guard.get(pid) {
                            let _ = tx.send(payload.clone());
                        }
                    }
                }
            }
        }
    }
}

async fn process_ws_event(
    sender_id: &str,
    text: &str,
    db: &mongodb::Database,
    conns: &WsConnections,
) {
    let Ok(ev) = serde_json::from_str::<serde_json::Value>(text) else { return; };
    match ev.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "message"     => ws_send_message(sender_id, &ev, db, conns).await,
        "typing"      => ws_forward_typing(sender_id, &ev, conns, true).await,
        "stop_typing" => ws_forward_typing(sender_id, &ev, conns, false).await,
        "read"        => ws_read_receipt(sender_id, &ev, db, conns).await,
        "delete"      => ws_delete_message(sender_id, &ev, db, conns).await,
        _             => {}
    }
}

async fn ws_send_message(
    sender_id: &str,
    ev: &serde_json::Value,
    db: &mongodb::Database,
    conns: &WsConnections,
) {
    let to      = match ev.get("to").and_then(|v| v.as_str())                             { Some(v) => v, None => return };
    let content = match ev.get("content").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) { Some(v) => v, None => return };
    let temp_id = ev.get("temp_id").and_then(|v| v.as_str()).unwrap_or("");
    let reply_to: Option<ChatReplyRef> = ev.get("reply_to")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let convs_coll = db.collection::<serde_json::Value>("conversations");
    let msgs_coll  = db.collection::<serde_json::Value>("chat_messages");
    let timestamp  = Utc::now().timestamp_millis();

    let conv_id = match convs_coll
        .find_one(doc! { "participants": { "$all": [sender_id, to], "$size": 2 } }, None)
        .await
    {
        Ok(Some(conv)) => conv
            .get("_id").and_then(|v| v.as_object())
            .and_then(|o| o.get("$oid")).and_then(|v| v.as_str())
            .unwrap_or_default().to_string(),
        _ => {
            let new_conv = json!({
                "participants": [sender_id, to],
                "last_message": content,
                "last_message_time": timestamp,
                "last_message_sender": sender_id,
                "unread": {},
            });
            match convs_coll.insert_one(new_conv, None).await {
                Ok(r) => r.inserted_id.as_object_id().map(|o| o.to_hex()).unwrap_or_default(),
                Err(_) => return,
            }
        }
    };

    let msg_doc = json!({
        "conversation_id": &conv_id,
        "sender_id": sender_id,
        "content": content,
        "timestamp": timestamp,
        "read": false,
        "deleted": false,
        "reply_to": reply_to,
    });
    let msg_id = match msgs_coll.insert_one(msg_doc, None).await {
        Ok(r) => r.inserted_id.as_object_id().map(|o| o.to_hex()).unwrap_or_default(),
        Err(_) => return,
    };

    let unread_key = format!("unread.{}", to);
    if let Ok(conv_oid) = ObjectId::parse_str(&conv_id) {
        let _ = convs_coll.update_one(
            doc! { "_id": conv_oid },
            doc! {
                "$set": { "last_message": content, "last_message_time": timestamp, "last_message_sender": sender_id },
                "$inc": { &unread_key: 1i32 },
            },
            None,
        ).await;
    }

    let msg_data = json!({
        "_id": &msg_id,
        "conversation_id": &conv_id,
        "sender_id": sender_id,
        "content": content,
        "timestamp": timestamp,
        "read": false,
        "deleted": false,
        "reply_to": reply_to,
    });

    let to_sender = serde_json::to_string(&json!({ "type": "sent",    "data": &msg_data, "temp_id": temp_id })).unwrap_or_default();
    let to_recip  = serde_json::to_string(&json!({ "type": "message", "data": &msg_data })).unwrap_or_default();

    let guard = conns.read().await;
    if let Some(tx) = guard.get(sender_id) { let _ = tx.send(to_sender); }
    if let Some(tx) = guard.get(to)        { let _ = tx.send(to_recip);  }
}

async fn ws_forward_typing(
    sender_id: &str,
    ev: &serde_json::Value,
    conns: &WsConnections,
    is_typing: bool,
) {
    let to = match ev.get("to").and_then(|v| v.as_str()) { Some(v) => v, None => return };
    let payload = serde_json::to_string(&json!({
        "type": if is_typing { "typing" } else { "stop_typing" },
        "from": sender_id,
    })).unwrap_or_default();
    let guard = conns.read().await;
    if let Some(tx) = guard.get(to) { let _ = tx.send(payload); }
}

async fn ws_read_receipt(
    reader_id: &str,
    ev: &serde_json::Value,
    db: &mongodb::Database,
    conns: &WsConnections,
) {
    let conv_id  = match ev.get("conversation_id").and_then(|v| v.as_str()) { Some(v) => v, None => return };
    let conv_oid = match ObjectId::parse_str(conv_id) { Ok(o) => o, Err(_) => return };

    let msgs_coll  = db.collection::<serde_json::Value>("chat_messages");
    let convs_coll = db.collection::<serde_json::Value>("conversations");

    let _ = msgs_coll.update_many(
        doc! { "conversation_id": conv_id, "sender_id": { "$ne": reader_id }, "read": false, "deleted": false },
        doc! { "$set": { "read": true } },
        None,
    ).await;

    let unread_key = format!("unread.{}", reader_id);
    let _ = convs_coll.update_one(doc! { "_id": &conv_oid }, doc! { "$set": { &unread_key: 0i32 } }, None).await;

    if let Ok(Some(conv)) = convs_coll.find_one(doc! { "_id": &conv_oid }, None).await {
        if let Some(parts) = conv.get("participants").and_then(|v| v.as_array()) {
            let payload = serde_json::to_string(&json!({
                "type": "read",
                "conversation_id": conv_id,
                "reader_id": reader_id,
            })).unwrap_or_default();
            let guard = conns.read().await;
            for p in parts {
                if let Some(pid) = p.as_str() {
                    if pid != reader_id {
                        if let Some(tx) = guard.get(pid) { let _ = tx.send(payload.clone()); }
                    }
                }
            }
        }
    }
}

async fn ws_delete_message(
    user_id: &str,
    ev: &serde_json::Value,
    db: &mongodb::Database,
    conns: &WsConnections,
) {
    let msg_id   = match ev.get("message_id").and_then(|v| v.as_str())      { Some(v) => v, None => return };
    let conv_id  = match ev.get("conversation_id").and_then(|v| v.as_str()) { Some(v) => v, None => return };
    let msg_oid  = match ObjectId::parse_str(msg_id)  { Ok(o) => o, Err(_) => return };
    let conv_oid = match ObjectId::parse_str(conv_id) { Ok(o) => o, Err(_) => return };

    let msgs_coll  = db.collection::<serde_json::Value>("chat_messages");
    let convs_coll = db.collection::<serde_json::Value>("conversations");

    let updated = msgs_coll.update_one(
        doc! { "_id": &msg_oid, "sender_id": user_id },
        doc! { "$set": { "deleted": true, "content": "This message was deleted" } },
        None,
    ).await.map(|r| r.modified_count > 0).unwrap_or(false);

    if !updated { return; }

    let payload = serde_json::to_string(&json!({
        "type": "deleted",
        "message_id": msg_id,
        "conversation_id": conv_id,
    })).unwrap_or_default();

    if let Ok(Some(conv)) = convs_coll.find_one(doc! { "_id": &conv_oid }, None).await {
        if let Some(parts) = conv.get("participants").and_then(|v| v.as_array()) {
            let guard = conns.read().await;
            for p in parts {
                if let Some(pid) = p.as_str() {
                    if let Some(tx) = guard.get(pid) { let _ = tx.send(payload.clone()); }
                }
            }
        }
    }
}

// ── WEBSOCKET UPGRADE HANDLER ────────────────────────────────────────────────

#[get("/ws/{user_id}")]
async fn ws_chat(
    req: HttpRequest,
    body: web::Payload,
    path: web::Path<String>,
    db: web::Data<mongodb::Database>,
    conns: web::Data<WsConnections>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = path.into_inner();
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    conns.write().await.insert(user_id.clone(), tx);

    {
        let users = db.collection::<serde_json::Value>("users");
        if let Ok(oid) = ObjectId::parse_str(&user_id) {
            let _ = users.update_one(doc! { "_id": oid }, doc! { "$set": { "online": true } }, None).await;
        }
    }
    send_online_status(&user_id, true, None, &db, &conns).await;

    let db    = db.into_inner();
    let conns = conns.into_inner();
    let uid   = user_id.clone();

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                msg = msg_stream.recv() => match msg {
                    Some(Ok(actix_ws::Message::Text(text))) => {
                        process_ws_event(&uid, text.as_ref(), &db, &conns).await;
                    }
                    Some(Ok(actix_ws::Message::Ping(bytes))) => {
                        let _ = session.pong(&bytes).await;
                    }
                    Some(Ok(actix_ws::Message::Close(_))) | None => break,
                    _ => {}
                },
                msg = rx.recv() => match msg {
                    Some(text) => { if session.text(text).await.is_err() { break; } }
                    None => break,
                },
            }
        }

        conns.write().await.remove(&uid);
        let now   = Utc::now().timestamp_millis();
        let users = db.collection::<serde_json::Value>("users");
        if let Ok(oid) = ObjectId::parse_str(&uid) {
            let _ = users.update_one(
                doc! { "_id": oid },
                doc! { "$set": { "online": false, "last_seen": now } },
                None,
            ).await;
        }
        send_online_status(&uid, false, Some(now), &db, &conns).await;
    });

    Ok(response)
}

// ── CHAT HTTP ENDPOINTS ──────────────────────────────────────────────────────

// HTTP fallback — saves message to DB even when WebSocket is down
// Also pushes to recipient via WS if they happen to be connected
#[post("/api/chat/send")]
async fn send_chat_message_http(
    body: web::Json<SendChatMessageRequest>,
    db: web::Data<mongodb::Database>,
    conns: web::Data<WsConnections>,
) -> HttpResponse {
    let convs_coll = db.collection::<serde_json::Value>("conversations");
    let msgs_coll  = db.collection::<serde_json::Value>("chat_messages");
    let timestamp  = Utc::now().timestamp_millis();

    if body.content.trim().is_empty() {
        return HttpResponse::BadRequest().json(json!({"error": "Empty message"}));
    }

    // Find existing conversation or create one
    let conv_id = match convs_coll
        .find_one(doc! { "participants": { "$all": [&body.sender_id, &body.to], "$size": 2 } }, None)
        .await
    {
        Ok(Some(conv)) => conv
            .get("_id").and_then(|v| v.as_object())
            .and_then(|o| o.get("$oid")).and_then(|v| v.as_str())
            .unwrap_or_default().to_string(),
        _ => {
            let new_conv = json!({
                "participants": [&body.sender_id, &body.to],
                "last_message": &body.content,
                "last_message_time": timestamp,
                "last_message_sender": &body.sender_id,
                "unread": {},
            });
            match convs_coll.insert_one(new_conv, None).await {
                Ok(r) => r.inserted_id.as_object_id().map(|o| o.to_hex()).unwrap_or_default(),
                Err(e) => {
                    eprintln!("Failed to create conversation: {}", e);
                    return HttpResponse::InternalServerError().json(json!({"error": "Failed to create conversation"}));
                }
            }
        }
    };

    // Save message to DB
    let msg_doc = json!({
        "conversation_id": &conv_id,
        "sender_id": &body.sender_id,
        "content": &body.content,
        "timestamp": timestamp,
        "read": false,
        "deleted": false,
        "reply_to": body.reply_to,
    });

    let msg_id = match msgs_coll.insert_one(msg_doc, None).await {
        Ok(r) => r.inserted_id.as_object_id().map(|o| o.to_hex()).unwrap_or_default(),
        Err(e) => {
            eprintln!("Failed to save chat message: {}", e);
            return HttpResponse::InternalServerError().json(json!({"error": "Failed to save message"}));
        }
    };

    // Update conversation preview + recipient unread count
    let unread_key = format!("unread.{}", body.to);
    if let Ok(oid) = ObjectId::parse_str(&conv_id) {
        let _ = convs_coll.update_one(
            doc! { "_id": oid },
            doc! {
                "$set": {
                    "last_message": &body.content,
                    "last_message_time": timestamp,
                    "last_message_sender": &body.sender_id,
                },
                "$inc": { &unread_key: 1i32 },
            },
            None,
        ).await;
    }

    let msg_data = json!({
        "_id": &msg_id,
        "conversation_id": &conv_id,
        "sender_id": &body.sender_id,
        "content": &body.content,
        "timestamp": timestamp,
        "read": false,
        "deleted": false,
        "reply_to": body.reply_to,
    });

    // If recipient is connected via WS, push it to them immediately
    let ws_payload = serde_json::to_string(&json!({ "type": "message", "data": &msg_data })).unwrap_or_default();
    let guard = conns.read().await;
    if let Some(tx) = guard.get(&body.to) {
        let _ = tx.send(ws_payload);
    }
    drop(guard);

    HttpResponse::Ok().json(json!({ "message": msg_data }))
}

#[get("/api/chat/users/{user_id}")]
async fn get_chat_users(
    user_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let uid        = user_id.into_inner();
    let users_coll = db.collection::<User>("users");

    let user_oid = match ObjectId::parse_str(&uid) {
        Ok(o) => o,
        Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})),
    };

    let current = match users_coll.find_one(doc! { "_id": &user_oid }, None).await {
        Ok(Some(u)) => u,
        _ => return HttpResponse::NotFound().json(json!({"error": "User not found"})),
    };

    let allowed: Vec<&str> = match current.role.as_deref().unwrap_or("") {
        "customer"   => vec!["management"],
        "management" => vec!["customer", "admin"],
        "admin"      => vec!["management", "customer"],
        _            => vec![],
    };

    if allowed.is_empty() {
        return HttpResponse::Ok().json(json!([]));
    }

    match users_coll.find(
        doc! { "role": { "$in": &allowed }, "_id": { "$ne": &user_oid } },
        None,
    ).await {
        Ok(mut cursor) => {
            let mut list = Vec::new();
            while let Ok(Some(u)) = cursor.try_next().await {
                list.push(json!({
                    "_id":        u.id.map(|o| o.to_hex()),
                    "firstName":  u.firstName,
                    "lastName":   u.lastName,
                    "role":       u.role,
                    "profilePic": u.profilePic,
                    "online":     u.online.unwrap_or(false),
                    "last_seen":  u.last_seen,
                }));
            }
            HttpResponse::Ok().json(list)
        }
        Err(e) => {
            eprintln!("Error fetching chat users: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch users"}))
        }
    }
}

#[get("/api/chat/conversations/{user_id}")]
async fn get_user_conversations(
    user_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let uid        = user_id.into_inner();
    let convs_coll = db.collection::<serde_json::Value>("conversations");
    let users_coll = db.collection::<User>("users");

    let opts = mongodb::options::FindOptions::builder()
        .sort(doc! { "last_message_time": -1 })
        .build();

    match convs_coll.find(doc! { "participants": &uid }, opts).await {
        Ok(mut cursor) => {
            let mut result = Vec::new();
            while let Ok(Some(conv)) = cursor.try_next().await {
                let conv_id = conv
                    .get("_id").and_then(|v| v.as_object())
                    .and_then(|o| o.get("$oid")).and_then(|v| v.as_str())
                    .unwrap_or_default().to_string();

                let unread = conv
                    .get("unread").and_then(|v| v.as_object())
                    .and_then(|o| o.get(&uid))
                    .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
                    .unwrap_or(0);

                if let Some(parts) = conv.get("participants").and_then(|v| v.as_array()) {
                    for p in parts {
                        if let Some(pid) = p.as_str() {
                            if pid != uid {
                                if let Ok(oid) = ObjectId::parse_str(pid) {
                                    if let Ok(Some(other)) = users_coll.find_one(doc! { "_id": oid }, None).await {
                                        result.push(json!({
                                            "conversation_id":      &conv_id,
                                            "other_user": {
                                                "_id":        pid,
                                                "firstName":  other.firstName,
                                                "lastName":   other.lastName,
                                                "role":       other.role,
                                                "profilePic": other.profilePic,
                                                "online":     other.online.unwrap_or(false),
                                                "last_seen":  other.last_seen,
                                            },
                                            "last_message":        conv.get("last_message"),
                                            "last_message_time":   conv.get("last_message_time"),
                                            "last_message_sender": conv.get("last_message_sender"),
                                            "unread": unread,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            HttpResponse::Ok().json(result)
        }
        Err(e) => {
            eprintln!("Error fetching conversations: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch conversations"}))
        }
    }
}

#[get("/api/chat/conversation/{conv_id}/messages")]
async fn get_conversation_messages(
    conv_id: web::Path<String>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let cid       = conv_id.into_inner();
    let msgs_coll = db.collection::<serde_json::Value>("chat_messages");

    let opts = mongodb::options::FindOptions::builder()
        .sort(doc! { "timestamp": 1 })
        .build();

    match msgs_coll.find(doc! { "conversation_id": &cid }, opts).await {
        Ok(mut cursor) => {
            let mut result: Vec<serde_json::Value> = Vec::new();
            while let Ok(Some(mut msg)) = cursor.try_next().await {
                if let Some(id_str) = msg.get("_id")
                    .and_then(|v| v.as_object())
                    .and_then(|o| o.get("$oid"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                {
                    if let Some(obj) = msg.as_object_mut() {
                        obj.insert("_id".to_string(), json!(id_str));
                    }
                }
                result.push(msg);
            }
            HttpResponse::Ok().json(result)
        }
        Err(e) => {
            eprintln!("Error fetching chat messages: {}", e);
            HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch messages"}))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EXISTING ENDPOINTS
// ═══════════════════════════════════════════════════════════════════════════

async fn signup(req: web::Json<SignupRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(_)) => return HttpResponse::BadRequest().json(ErrorResponse { error: "Email already exists".to_string() }),
        Err(e) => { println!("DB error: {}", e); return HttpResponse::InternalServerError().json(ErrorResponse { error: "Database error".to_string() }); }
        Ok(None) => {}
    }
    let hashed = bcrypt::hash(&req.password, 4).unwrap_or_else(|_| req.password.clone());
    let now_timestamp = Utc::now().timestamp_millis();
    let user = User {
        id: None, firstName: req.firstName.clone(), lastName: req.lastName.clone(),
        email: req.email.clone(), password: hashed, role: None,
        profilePic: None, backgroundImage: None, coverImage: None,
        phone: None, location: None, website: None, bio: None,
        createdAt: Some(now_timestamp), online: None, last_seen: None,
    };
    match users.insert_one(&user, None).await {
        Ok(result) => {
            let user_id = result.inserted_id.as_object_id().map(|oid| oid.to_hex()).unwrap_or_else(|| "unknown".to_string());
            let welcome_email = json!({
                "user_id": &user_id, "id": "welcome-email", "from": "Dispatch Team",
                "subject": "Welcome to Dispatch! 🎉",
                "preview": "Get started with your incident reporting dashboard",
                "body": "Welcome to Dispatch! 🎉\nThank you for creating your account.\n\nBest regards,\nThe Dispatch Team",
                "timestamp": "just now", "createdAtTimestamp": now_timestamp,
                "unread": true, "starred": false, "role": "admin", "isWelcome": true
            });
            let messages = db.collection::<serde_json::Value>("messages");
            let _ = messages.insert_one(welcome_email, None).await;
            println!("User created: {} — ID: {}", req.email, user_id);
            HttpResponse::Ok().json(AuthResponse {
                message: "User created".to_string(), user_id,
                firstName: req.firstName.clone(), lastName: req.lastName.clone(), email: req.email.clone(),
                createdAt: format_timestamp(now_timestamp), createdAtRelative: format_timestamp_relative(now_timestamp),
            })
        }
        Err(e) => { println!("Error inserting user: {}", e); HttpResponse::BadRequest().json(ErrorResponse { error: "Failed to create user".to_string() }) }
    }
}

async fn login(req: web::Json<LoginRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(user)) => {
            if bcrypt::verify(&req.password, &user.password).unwrap_or(false) {
                let user_id = user.id.map(|oid| oid.to_hex()).unwrap_or_else(|| "unknown".to_string());
                HttpResponse::Ok().json(LoginResponse {
                    message: "Login successful".to_string(), user_id,
                    firstName: user.firstName, lastName: user.lastName, email: user.email, role: user.role,
                    createdAt: user.createdAt.map(format_timestamp).unwrap_or_else(|| "Unknown".to_string()),
                    createdAtRelative: user.createdAt.map(format_timestamp_relative).unwrap_or_else(|| "Unknown".to_string()),
                })
            } else {
                HttpResponse::Unauthorized().json(ErrorResponse { error: "Invalid password".to_string() })
            }
        }
        Ok(None) => HttpResponse::Unauthorized().json(ErrorResponse { error: "User not found".to_string() }),
        Err(e) => { println!("DB error: {}", e); HttpResponse::InternalServerError().json(ErrorResponse { error: "Database error".to_string() }) }
    }
}

async fn set_role(req: web::Json<SetRoleRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let user_oid = match ObjectId::parse_str(&req.user_id) {
        Ok(oid) => oid,
        Err(e) => { println!("Invalid user_id: {} - {}", req.user_id, e); return HttpResponse::BadRequest().json(ErrorResponse { error: "Invalid user ID format".to_string() }); }
    };
    match users.update_one(doc! { "_id": user_oid }, doc! { "$set": { "role": &req.role } }, None).await {
        Ok(result) => {
            if result.modified_count > 0 { HttpResponse::Ok().json(json!({ "message": format!("Role set to {}", req.role) })) }
            else { HttpResponse::BadRequest().json(ErrorResponse { error: "User not found".to_string() }) }
        }
        Err(e) => { println!("Error: {}", e); HttpResponse::BadRequest().json(ErrorResponse { error: "Failed to set role".to_string() }) }
    }
}

#[get("/api/user/{user_id}")]
async fn get_user(user_id: web::Path<String>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let uid = user_id.into_inner();
    let oid = match ObjectId::parse_str(&uid) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})) };
    match users.find_one(doc! { "_id": oid }, None).await {
        Ok(Some(user)) => {
            let mut r = json!({
                "_id": user.id.map(|o| o.to_hex()), "firstName": user.firstName, "lastName": user.lastName,
                "email": user.email, "role": user.role, "profilePic": user.profilePic,
                "backgroundImage": user.backgroundImage, "coverImage": user.coverImage,
                "phone": user.phone, "location": user.location, "website": user.website, "bio": user.bio,
                "online": user.online.unwrap_or(false), "last_seen": user.last_seen,
            });
            if let Some(ts) = user.createdAt {
                r["createdAt"]         = json!(format_timestamp(ts));
                r["createdAtRelative"] = json!(format_timestamp_relative(ts));
            }
            HttpResponse::Ok().json(r)
        }
        Ok(None) => HttpResponse::NotFound().json(json!({"error": "User not found"})),
        Err(e)   => { eprintln!("DB error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Database error"})) }
    }
}

#[post("/api/user/{user_id}/profile-pic")]
async fn save_profile_pic(user_id: web::Path<String>, body: web::Json<ProfilePicRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let oid = match ObjectId::parse_str(&user_id.into_inner()) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})) };
    match users.update_one(doc! { "_id": oid }, doc! { "$set": { "profilePic": &body.profilePic } }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Profile pic saved"})),
        Err(e) => { eprintln!("DB error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to save profile pic"})) }
    }
}

#[post("/api/user/{user_id}/background")]
async fn save_background(user_id: web::Path<String>, body: web::Json<BackgroundRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let oid = match ObjectId::parse_str(&user_id.into_inner()) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})) };
    match users.update_one(doc! { "_id": oid }, doc! { "$set": { "backgroundImage": &body.backgroundImage } }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Background image saved"})),
        Err(e) => { eprintln!("DB error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to save background"})) }
    }
}

#[post("/api/user/{user_id}/cover-image")]
async fn save_cover_image(user_id: web::Path<String>, body: web::Json<CoverImageRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let oid = match ObjectId::parse_str(&user_id.into_inner()) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})) };
    match users.update_one(doc! { "_id": oid }, doc! { "$set": { "coverImage": &body.coverImage } }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Cover image saved"})),
        Err(e) => { eprintln!("DB error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to save cover image"})) }
    }
}

#[post("/api/user/{user_id}/profile")]
async fn update_profile(user_id: web::Path<String>, body: web::Json<ProfileUpdateRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    let oid = match ObjectId::parse_str(&user_id.into_inner()) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid user ID"})) };
    let mut fields = doc! {};
    if let Some(ref v) = body.firstName { fields.insert("firstName", v); }
    if let Some(ref v) = body.lastName  { fields.insert("lastName",  v); }
    if let Some(ref v) = body.bio       { fields.insert("bio",       v); }
    if let Some(ref v) = body.phone     { fields.insert("phone",     v); }
    if let Some(ref v) = body.location  { fields.insert("location",  v); }
    if let Some(ref v) = body.website   { fields.insert("website",   v); }
    if let Some(ref v) = body.role      { fields.insert("role",      v); }
    match users.update_one(doc! { "_id": oid }, doc! { "$set": fields }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Profile updated"})),
        Err(e) => { eprintln!("DB error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to update profile"})) }
    }
}

#[post("/api/user/{user_id}/message/{message_id}/read")]
async fn mark_message_read(path: web::Path<(String, String)>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let (uid, mid) = path.into_inner();
    match col.update_one(doc! { "user_id": &uid, "id": &mid }, doc! { "$set": { "unread": false } }, None).await {
        Ok(r) => { if r.modified_count > 0 { HttpResponse::Ok().json(json!({"message": "Marked as read"})) } else { HttpResponse::Ok().json(json!({"message": "Not found"})) } }
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed"})) }
    }
}

#[get("/api/user/{user_id}/messages")]
async fn get_user_messages(user_id: web::Path<String>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let uid = user_id.into_inner();
    match col.find(doc! { "user_id": &uid }, None).await {
        Ok(mut cursor) => {
            let mut msgs = Vec::new();
            while let Ok(Some(m)) = cursor.try_next().await { msgs.push(m); }
            HttpResponse::Ok().json(msgs)
        }
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch messages"})) }
    }
}

#[post("/api/user/{user_id}/messages")]
async fn save_messages(user_id: web::Path<String>, body: web::Json<SaveMessagesRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let uid = user_id.into_inner();
    col.delete_many(doc! { "user_id": &uid }, None).await.ok();
    for msg in &body.messages {
        let mut m = msg.clone();
        m["user_id"] = json!(uid.clone());
        m["createdAt"] = json!(Utc::now().timestamp_millis());
        col.insert_one(m, None).await.ok();
    }
    HttpResponse::Ok().json(json!({"message": "Messages saved successfully"}))
}

#[post("/api/user/{user_id}/message/{message_id}")]
async fn update_message(path: web::Path<(String, String)>, body: web::Json<MessageUpdateRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let (uid, mid) = path.into_inner();
    let mut fields = doc! {};
    if let Some(read)    = body.read    { fields.insert("read",    read); }
    if let Some(starred) = body.starred { fields.insert("starred", starred); }
    if fields.is_empty() { return HttpResponse::BadRequest().json(json!({"error": "No fields to update"})); }
    let oid = match ObjectId::parse_str(&mid) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid message ID"})) };
    match col.update_one(doc! { "user_id": &uid, "_id": oid }, doc! { "$set": fields }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Updated"})),
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed"})) }
    }
}

#[delete("/api/user/{user_id}/message/{message_id}")]
async fn delete_message(path: web::Path<(String, String)>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let (uid, mid) = path.into_inner();
    if let Ok(r) = col.delete_one(doc! { "user_id": &uid, "id": &mid }, None).await {
        if r.deleted_count > 0 { return HttpResponse::Ok().json(json!({"message": "Deleted"})); }
    }
    let oid = match ObjectId::parse_str(&mid) { Ok(o) => o, Err(_) => return HttpResponse::BadRequest().json(json!({"error": "Invalid message ID"})) };
    match col.delete_one(doc! { "user_id": &uid, "_id": oid }, None).await {
        Ok(_) => HttpResponse::Ok().json(json!({"message": "Deleted"})),
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed"})) }
    }
}

#[get("/api/users")]
async fn get_all_users(db: web::Data<mongodb::Database>) -> HttpResponse {
    let users = db.collection::<User>("users");
    match users.find(doc! {}, None).await {
        Ok(mut cursor) => {
            let mut list = Vec::new();
            while let Ok(Some(u)) = cursor.try_next().await {
                list.push(json!({ "_id": u.id.map(|o| o.to_hex()), "firstName": u.firstName, "lastName": u.lastName, "email": u.email, "role": u.role }));
            }
            HttpResponse::Ok().json(list)
        }
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed to fetch users"})) }
    }
}

#[post("/api/admin/send-email")]
async fn send_email_to_users(body: web::Json<SendEmailRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("messages");
    let mut sent = 0;
    for uid in &body.userIds {
        let ts = Utc::now().timestamp_millis();
        let email = json!({
            "user_id": uid, "id": format!("admin-email-{}", ts),
            "from": &body.from, "subject": &body.subject,
            "preview": body.body.chars().take(100).collect::<String>(),
            "body": &body.body, "timestamp": "just now", "createdAtTimestamp": ts,
            "unread": true, "starred": false, "role": "admin", "isAdmin": true
        });
        if col.insert_one(email, None).await.is_ok() { sent += 1; }
    }
    HttpResponse::Ok().json(json!({ "message": "Email sent", "count": sent, "requested": body.userIds.len() }))
}

#[get("/api/user/{user_id}/viewed-emails")]
async fn get_viewed_emails(user_id: web::Path<String>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("viewed_emails");
    let uid = user_id.into_inner();
    match col.find_one(doc! { "user_id": &uid }, None).await {
        Ok(Some(doc)) => {
            let ids: Vec<String> = doc.get("emailIds").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            HttpResponse::Ok().json(json!({"viewedEmails": ids}))
        }
        Ok(None) => HttpResponse::Ok().json(json!({"viewedEmails": []})),
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed"})) }
    }
}

#[post("/api/user/{user_id}/viewed-emails")]
async fn mark_email_viewed(user_id: web::Path<String>, body: web::Json<ViewedEmailRequest>, db: web::Data<mongodb::Database>) -> HttpResponse {
    let col = db.collection::<serde_json::Value>("viewed_emails");
    let uid = user_id.into_inner();
    match col.update_one(doc! { "user_id": &uid }, doc! { "$addToSet": { "emailIds": &body.emailId } }, None).await {
        Ok(r) => {
            if r.modified_count == 0 {
                let _ = col.insert_one(json!({ "user_id": &uid, "emailIds": vec![&body.emailId] }), None).await;
            }
            HttpResponse::Ok().json(json!({"message": "Marked as viewed"}))
        }
        Err(e) => { eprintln!("Error: {}", e); HttpResponse::InternalServerError().json(json!({"error": "Failed"})) }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let mongo_url = std::env::var("MONGO_URL").expect("MONGO_URL not set");
    let client_options = ClientOptions::parse(&mongo_url).await.expect("Failed to parse MongoDB URL");
    let client = Client::with_options(client_options).expect("Failed to create MongoDB client");
    let db     = client.database("my_app");

    let users = db.collection::<User>("users");
    match users.create_index(
        mongodb::IndexModel::builder()
            .keys(doc! { "email": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build(),
        None,
    ).await {
        Ok(_)  => println!("✓ Email unique index verified"),
        Err(e) => println!("⚠ Could not create index: {}", e),
    }

    let db              = web::Data::new(db);
    let connections: WsConnections = Arc::new(RwLock::new(HashMap::new()));
    let connections_data = web::Data::new(connections);

    println!("🚀 Server starting on http://0.0.0.0:8000");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);

        App::new()
            .app_data(db.clone())
            .app_data(connections_data.clone())
            .wrap(cors)
            .wrap(middleware::NormalizePath::trim())
            // Auth
            .route("/api/signup",   web::post().to(signup))
            .route("/api/login",    web::post().to(login))
            .route("/api/set-role", web::post().to(set_role))
            // User
            .service(get_user)
            .service(save_profile_pic)
            .service(save_background)
            .service(save_cover_image)
            .service(update_profile)
            // Inbox
            .service(mark_message_read)
            .service(get_user_messages)
            .service(save_messages)
            .service(update_message)
            .service(delete_message)
            // Viewed emails
            .service(get_viewed_emails)
            .service(mark_email_viewed)
            // Admin
            .service(get_all_users)
            .service(send_email_to_users)
            // Chat
            .service(ws_chat)
            .service(send_chat_message_http)   // ← HTTP fallback for when WS is down
            .service(get_chat_users)
            .service(get_user_conversations)
            .service(get_conversation_messages)
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}