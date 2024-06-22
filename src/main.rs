extern crate core;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use base64::Engine;
use chrono::Utc;

use base64::engine::general_purpose;
use image::io::Reader as ImageReader;
use lambda_http::{Error, lambda_runtime, LambdaEvent, service_fn, tracing};
use lambda_http::aws_lambda_events::apigw::{ApiGatewayProxyRequest, ApiGatewayProxyResponse};
use usls::{Annotator, Options};
use usls::models::{YOLO, YOLOVersion};

async fn handler(model: Rc<RefCell<YOLO>>, event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<ApiGatewayProxyResponse, Error> {

    let image_data = general_purpose::STANDARD.decode(&event.payload.body.unwrap()).unwrap();
    let img = vec![ImageReader::new(std::io::Cursor::new(image_data))
        .with_guessed_format()?.decode()?];

    let mut model = model.borrow_mut();
    let y = model.run(&img)?;

    // annotate
    let annotator = Annotator::default().with_saveout("YOLOv10");
    annotator.annotate(&img, &y);

    let response_body = format!("Image processed successfully at {}", Utc::now().to_rfc3339());
    log::info!("{}", response_body);

    //let dimensions = img.dimensions();
    let response = ApiGatewayProxyResponse {
        status_code: 200,
        headers: Default::default(),
        multi_value_headers: Default::default(),
        body: Some(response_body.into()),
        is_base64_encoded: false,
    };

    Ok(response)
}


#[tokio::main]
async fn main() -> Result<(), Error> {

    tracing::init_default_subscriber();

    log::info!("Loading model");

    let options = Options::default()
        .with_coreml(0)
        .with_dry_run(0)
        .with_model(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("model")
                .join("yolov10x.onnx").to_str().unwrap())?
        .with_yolo_version(YOLOVersion::V10)
        .with_i00((1, 1, 4).into())
        .with_i02((416, 640, 800).into())
        .with_i03((416, 640, 800).into())
        .with_confs(&[0.4, 0.15]);


    let model = Rc::new(RefCell::new( YOLO::new(options).unwrap()));

    log::info!("{}: Finished loading model ", std::thread::current().name().unwrap());

    lambda_runtime::run(
        service_fn(move |event: LambdaEvent<ApiGatewayProxyRequest>| {
            let model = Rc::clone(&model);
            async move {
                handler(model, event).await
            }
        })
    ).await
}