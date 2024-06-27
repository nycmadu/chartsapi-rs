#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use crate::faa_metafile::DigitalTpp;
use crate::response_dtos::ChartDto;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use indexmap::IndexMap;
use quick_xml::de::from_str;
use serde::Deserialize;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

mod faa_metafile;
mod response_dtos;

type ChartsHashMap = IndexMap<String, Vec<ChartDto>>;

struct ChartsHashMaps {
    faa: ChartsHashMap,
    icao: ChartsHashMap,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let hashmaps = Arc::new(load_charts().await.unwrap());

    let app = Router::new()
        .route("/v1/charts", get(charts_handler))
        .with_state(hashmaps)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct ChartsAirports {
    apt: String,
}

async fn charts_handler(
    State(hashmaps): State<Arc<ChartsHashMaps>>,
    airports: Query<ChartsAirports>,
) -> impl IntoResponse {
    let Query(airports_str) = airports;
    let mut results: IndexMap<String, Vec<ChartDto>> = IndexMap::new();
    for airport in airports_str.apt.split(',') {
        if let Some(charts) = lookup_charts(airport, &hashmaps) {
            results.insert(airport.to_owned(), charts.clone());
        }
    }
    Json(results)
}

fn lookup_charts<'a>(apt_id: &str, hashmaps: &'a Arc<ChartsHashMaps>) -> Option<&'a Vec<ChartDto>> {
    hashmaps
        .faa
        .get(&apt_id.to_uppercase())
        .or_else(|| hashmaps.icao.get(&apt_id.to_uppercase()))
}

async fn load_charts() -> Result<ChartsHashMaps, anyhow::Error> {
    let metafile = reqwest::get("https://aeronav.faa.gov/d-tpp/2406/xml_data/d-tpp_Metafile.xml")
        .await?
        .text()
        .await?;
    let dtpp = from_str::<DigitalTpp>(&metafile)?;
    let mut faa: ChartsHashMap = IndexMap::new();
    let mut icao: ChartsHashMap = IndexMap::new();
    let mut count = 0;

    for state in dtpp.states {
        for city in state.cities {
            for airport in city.airports {
                for record in airport.chart_records {
                    let chart_dto = ChartDto {
                        state: state.id.clone(),
                        state_full: state.full_name.clone(),
                        city: city.id.clone(),
                        volume: city.volume.clone(),
                        airport_name: airport.id.clone(),
                        military: airport.military.clone(),
                        faa_ident: airport.apt_ident.clone(),
                        icao_ident: airport.icao_ident.clone(),
                        chart_seq: record.chartseq.clone(),
                        chart_code: record.chart_code.clone(),
                        chart_name: record.chart_name.clone(),
                        pdf_name: record.pdf_name.clone(),
                        pdf_path: format!(
                            "https://aeronav.faa.gov/d-tpp/2406/{pdf}",
                            pdf = record.pdf_name
                        ),
                    };

                    faa.entry(chart_dto.faa_ident.clone())
                        .and_modify(|charts| charts.push(chart_dto.clone()))
                        .or_insert(vec![chart_dto.clone()]);

                    icao.entry(chart_dto.icao_ident.clone())
                        .and_modify(|charts| charts.push(chart_dto.clone()))
                        .or_insert(vec![chart_dto.clone()]);

                    count += 1;
                }
            }
        }
    }

    info!("Loaded {num} charts", num = count);
    Ok(ChartsHashMaps { faa, icao })
}
