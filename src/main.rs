use actix_web::{HttpServer, HttpResponse, web, App, Responder, post};
use std::sync::mpsc;
use tokio::sync::oneshot;

mod engine;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DepositRequest {
    user_id: String,
    asset: String,
    amount: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetBalanceRequest {
    user_id: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    
    let (tx, rx) = mpsc::channel();

    std::thread::spawn( move || {
        println!("inside the new OS thread");
        engine::run(rx);
    });

    HttpServer::new( move || {
        App::new()
            .app_data(web::Data::new(tx.clone()))
            .service(hello)
            .service(initialize_user)
            .service(deposit)
            .service(get_balances)
            .service(create_order)
            .service(cancel_order)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[post("/hello")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("hello there")
}

#[post("/initialize_user")]
async fn initialize_user(tx: web::Data<mpsc::Sender<engine::EngineCommand>>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();

    tx.send(engine::EngineCommand::InitializeUser {
        tx_oneshot
    }).unwrap();

    match rx.await {
        Ok(msg) => HttpResponse::Ok().body(format!("engine replied: {}", msg)),
        Err(_) => HttpResponse::InternalServerError().body("engine did not respond"),
    }
}

#[post("/deposit")]
async fn deposit(tx: web::Data<mpsc::Sender<engine::EngineCommand>>, body: web::Json<DepositRequest>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::Deposit {
        user_id: Uuid::parse_str(&body.user_id).unwrap(),
        asset: body.asset.clone(),
        amount: body.amount,
        tx_oneshot
    }).unwrap();

    match rx.await {
        Ok(msg) => HttpResponse::Ok().json(serde_json::json!({
            "message": msg,
        })),
        Err(_) => HttpResponse::InternalServerError().body("engine failed to respond"),
    }
}

#[post("/get_balances")]
async fn get_balances(tx: web::Data<mpsc::Sender<engine::EngineCommand>>, body: web::Json<GetBalanceRequest>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::GetBalances {
        user_id: Uuid::parse_str(&body.user_id).unwrap(),
        tx_oneshot
    }).unwrap();

    match rx.await {
        Ok(Some(user_balance)) => HttpResponse::Ok().json(serde_json::json!({
            "user_id": user_id,
            "balances": user_balance
        })),
        Ok(None) => HttpResponse::Ok().json(serde_json::json!({
            "error": "user not found"
        })),
        Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "engine failed to respond"
        }))

    }
    
}

#[post("/create_order")]
async fn create_order(tx: web::Data<mpsc::Sender<engine::EngineCommand>>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::CreateOrder {
        tx_oneshot
    }).unwrap();

    match rx.await {
        Ok(msg) => HttpResponse::Ok().body(format!("engine replied: {}", msg)),
        Err(_) => HttpResponse::InternalServerError().body("engine failed to respond"),
    }
}

#[post("/cancel_order")]
async fn cancel_order(tx: web::Data<mpsc::Sender<engine::EngineCommand>>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::CancelOrder {
        tx_oneshot
    }).unwrap();
    match rx.await {
        Ok(msg) => HttpResponse::Ok().body(format!("engine replied: {}", msg)),
        Err(_) => HttpResponse::InternalServerError().body("engine failed to respond"),
    }
    
}
