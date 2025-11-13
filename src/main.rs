use actix_web::{HttpServer, HttpResponse, web, App, Responder, post};
use std::sync::mpsc;
use tokio::sync::oneshot;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

mod engine;
use engine::orderbook::Side;

mod math;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DepositRequest {
    user_id: String,
    asset: String,
    amount: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetBalanceRequest {
    user_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CreateOrderRequest {
    user_id: String,
    side: String,
    price: String,
    quantity: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CancelOrderRequest {
    user_id: String,
    order_id: String,
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

    let amount = match body.asset.to_uppercase().as_str() {
        "BTC" => match math::btc_to_sats_str(&body.amount) {
            Ok(v) => v,
            Err(e) => return HttpResponse::BadRequest().body(e),
        },
        "USDC" => match math::usdc_to_micro_usdc(&body.amount) {
            Ok(v) => v,
            Err(e) => return HttpResponse::BadRequest().body(e),
        },
        _ => return HttpResponse::BadRequest().body("unknown asset"),
    };

    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::Deposit {
        user_id: Uuid::parse_str(&body.user_id).unwrap(),
        asset: body.asset.to_uppercase(),
        amount,
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
         Ok(Some(user_balance)) => HttpResponse::Ok().json(user_balance),
         Ok(None) => HttpResponse::Ok().json(serde_json::json!({
             "error": "user not found"
         })),
         Err(_) => HttpResponse::InternalServerError().json(serde_json::json!({
             "error": "engine failed to respond"
         }))
     }
    
}

#[post("/create_order")]
async fn create_order(tx: web::Data<mpsc::Sender<engine::EngineCommand>>, body: web::Json<CreateOrderRequest>) -> impl Responder {
    let (tx_oneshot, rx) = oneshot::channel();

    let side = match body.side.to_lowercase().as_str() {
        "bid" => Side::Bid,
        "ask" => Side::Ask,
        _ => return HttpResponse::BadRequest().body("invalid side"),
    };

    let price = match math::price_to_micro_str(&body.price) {
        Ok(v) => v,
        Err(e) => return HttpResponse::BadRequest().body(e),
    };
    let quantity = match math::btc_to_sats_str(&body.quantity) {
        Ok(v) => v,
        Err(e) => return HttpResponse::BadRequest().body(e),
    };

    tx.send(engine::EngineCommand::CreateOrder {
        user_id: Uuid::parse_str(&body.user_id).unwrap(),
        side,
        price,
        quantity,
        tx_oneshot,
    }).unwrap();

    match rx.await {
        Ok(order_id) => HttpResponse::Ok().json(serde_json::json!({
            "msg": "order was created successfully",
            "order_id": order_id
        })),
        Err(_) => HttpResponse::InternalServerError().body("engine failed to respond"),
    }
}

#[post("/cancel_order")]
async fn cancel_order(tx: web::Data<mpsc::Sender<engine::EngineCommand>>, body: web::Json<CancelOrderRequest>) -> impl Responder {
    let user_id = match Uuid::parse_str(&body.user_id) {
        Ok(v) => v,
        Err(_) => return HttpResponse::BadRequest().body("invalid user id"),
    };
    let order_id = match Uuid::parse_str(&body.order_id) {
        Ok(v) => v,
        Err(_) => return HttpResponse::BadRequest().body("invalid order id"),
    };

    let (tx_oneshot, rx) = oneshot::channel();
    tx.send(engine::EngineCommand::CancelOrder {
        user_id,
        order_id,
        tx_oneshot
    }).unwrap();
    
    match rx.await {
        Ok(msg) => HttpResponse::Ok().json(serde_json::json!({
            "msg": msg
        })),
        Err(_) => HttpResponse::InternalServerError().body("engine failed to cancel order"),
    }
}
