extern crate core;

use aws_lambda_events::apigw::ApiGatewayProxyResponse;
use aws_lambda_events::encodings::Body;
use aws_lambda_events::http::HeaderMap;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::io::Reader as ImageReader;
use image::{DynamicImage, ImageFormat};
use lambda_http::aws_lambda_events::apigw::ApiGatewayProxyRequest;
use lambda_http::{lambda_runtime, service_fn, tracing, Error, LambdaEvent};
use log::{debug, error, info};
use std::cell::RefCell;
use std::env;
use std::io::Cursor;
use std::rc::Rc;
use std::time::Instant;
use tracing_subscriber::fmt;
use usls::models::{YOLOVersion, YOLO};
use usls::{Annotator, Options, Vision, Y};

#[tokio::main]
async fn main() -> Result<(), Error> {
    setup_logger();
    info!("Starting lambda runtime");

    let model_path = env::var("MODEL_PATH").expect("MODEL_PATH not set");
    let handler = Rc::new(RefCell::new(Handler::new(&model_path)?));

    // Run the lambda runtime with the closure that includes the handler
    lambda_runtime::run(service_fn(
        move |event: LambdaEvent<ApiGatewayProxyRequest>| {
            let handler = handler.clone();
            async move { handler.borrow_mut().handle(event).await }
        },
    ))
    .await
}

pub struct Handler {
    model: YOLO,
}
impl Handler {
    pub fn new(model_path: &String) -> Result<Self, Error> {
        let model = Handler::create_model(model_path)?;
        Ok(Self { model })
    }

    fn create_model(model_path: &String) -> Result<YOLO, Error> {
        debug!("Loading model from path: {}", model_path);

        let start_time = Instant::now();

        let model = match YOLO::new(
            Options::default()
                .with_cpu()
                .with_model(model_path)?
                .with_profile(false)
                .with_fp16(true)
                .with_dry_run(0)
                .with_yolo_version(YOLOVersion::V10)
                .with_i00((1, 1, 4).into())
                .with_i02((416, 640, 800).into())
                .with_i03((416, 640, 800).into())
                .with_confs(&[0.4, 0.15]),
        ) {
            Ok(model) => model,
            Err(e) => {
                info!("Failed to create model from path: {}", model_path);
                return Err(Error::from(e));
            }
        };

        info!(
            "Model created successfully from path: {} in {:?} took",
            model_path,
            start_time.elapsed()
        );
        Ok(model)
    }

    fn extract_image_from_body(
        &self,
        event: LambdaEvent<ApiGatewayProxyRequest>,
    ) -> Result<Vec<DynamicImage>, Error> {
        info!("Received event, Loading image");

        let b64_image_data = event.payload.body.as_ref().ok_or_else(|| {
            error!("No body found in the request");
            Error::from("No body found in the request")
        })?;

        let image_data = STANDARD.decode(b64_image_data).map_err(|e| {
            error!("Error decoding image: {}", e);
            Error::from(e)
        })?;
        info!("Image loaded successfully");

        let img = ImageReader::new(Cursor::new(image_data))
            .with_guessed_format()
            .map_err(|e| {
                error!("Error guessing the image format: {}", e);
                Error::from(e)
            })?
            .decode()
            .map_err(|e| {
                error!("Error decoding the image: {}", e);
                Error::from(e)
            })?;

        info!("Successfully extracted image from body");
        Ok(vec![img]) // Return a vector of images
    }

    fn infer(&mut self, img: &[DynamicImage]) -> Result<Vec<Y>, Error> {
        info!("Running model");
        let start_time = Instant::now();
        let y = self.model.run(img)?;
        info!("Model ran successfully in {:?}", start_time.elapsed());
        Ok(y)
    }

    fn annotate_img_with_result(&self, img: &[DynamicImage], y: Vec<Y>) -> DynamicImage {
        info!("Annotating image");
        let annotator = Annotator::default().with_saveout("/tmp/YOLOv10");
        annotator.annotate_image(&img[0], &y[0])
    }

    pub async fn handle(
        &mut self,
        event: LambdaEvent<ApiGatewayProxyRequest>,
    ) -> Result<ApiGatewayProxyResponse, Error> {
        let img = match self.extract_image_from_body(event) {
            Ok(image) => {
                info!("Successfully extracted image with size: {:?}", image.len());
                image
            }
            Err(e) => {
                error!("Error extracting image: {}", e);
                return Err(e);
            }
        };

        let res = self.infer(&img)?;
        let annotated_img = self.annotate_img_with_result(&img, res);

        info!("Encoding image b64");
        let mut image_data: Vec<u8> = Vec::new();
        DynamicImage::ImageRgb8(annotated_img.to_rgb8())
            .write_to(&mut Cursor::new(&mut image_data), ImageFormat::Jpeg)?;
        let encoded_image = STANDARD.encode(&image_data);
        info!("Image encoded successfully");

        info!(
            "Annotated image size: {} and encoded image size: {}",
            image_data.len(),
            encoded_image.len()
        );

        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "image/jpeg".parse().unwrap());

        let response = ApiGatewayProxyResponse {
            status_code: 200,
            headers,
            multi_value_headers: Default::default(),
            body: Body::Text(encoded_image).into(),
            is_base64_encoded: true,
        };

        Ok(response)
    }
}

fn setup_logger() {
    tracing_subscriber::fmt()
        // Customize the format to exclude the tracing information
        .fmt_fields(fmt::format::DefaultFields::new())
        .with_max_level(tracing::Level::INFO)
        .event_format(fmt::format().compact())
        .init();
}
