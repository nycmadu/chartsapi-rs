#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use crate::faa_metafile::DigitalTpp;
use crate::response_dtos::{ChartDto, ChartGroup};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use indexmap::IndexMap;
use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
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
struct ChartsOptions {
    apt: Option<String>,
    group: Option<i32>,
}

#[derive(Serialize, Deserialize)]
struct ErrorMessage {
    pub status: &'static str,
    pub status_code: &'static str,
    pub message: &'static str,
}

async fn charts_handler(
    State(hashmaps): State<Arc<ChartsHashMaps>>,
    options: Query<ChartsOptions>,
) -> Response {
    let Query(chart_options) = options;

    // Check that we have an airport to lookup
    if chart_options.apt.is_none()
        || chart_options
            .apt
            .as_ref()
            .is_some_and(|s| s.trim().is_empty())
    {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorMessage {
                status: "error",
                status_code: "404",
                message: "Please specify an airport.",
            }),
        )
            .into_response();
    }

    // Check if supplied chart group is valid, if given as param
    if chart_options.group.is_some_and(|i| !(1..=7).contains(&i)) {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorMessage {
                status: "error",
                status_code: "403",
                message: "That is not a valid grouping code.",
            }),
        )
            .into_response();
    }

    let mut results: IndexMap<String, Vec<ChartDto>> = IndexMap::new();
    for airport in chart_options.apt.unwrap().split(',') {
        if let Some(charts) = lookup_charts(airport, &hashmaps) {
            results.insert(
                airport.to_owned(),
                filter_charts_group(charts, chart_options.group),
            );
        }
    }
    (StatusCode::OK, Json(results)).into_response()
}

fn lookup_charts<'a>(apt_id: &str, hashmaps: &'a Arc<ChartsHashMaps>) -> Option<&'a Vec<ChartDto>> {
    hashmaps
        .faa
        .get(&apt_id.to_uppercase())
        .or_else(|| hashmaps.icao.get(&apt_id.to_uppercase()))
}

fn filter_charts_group(charts: &[ChartDto], group: Option<i32>) -> Vec<ChartDto> {
    group.map_or_else(
        || charts.to_owned(),
        |i| {
            charts
                .iter()
                .filter(|c| match i {
                    1 => matches!(
                        c.chart_group,
                        ChartGroup::General
                            | ChartGroup::APD
                            | ChartGroup::Departures
                            | ChartGroup::Arrivals
                            | ChartGroup::Approaches
                    ),
                    2 => matches!(c.chart_group, ChartGroup::APD),
                    3 => matches!(c.chart_group, ChartGroup::APD | ChartGroup::General),
                    4 => matches!(c.chart_group, ChartGroup::Departures),
                    5 => matches!(c.chart_group, ChartGroup::Arrivals),
                    6 => matches!(c.chart_group, ChartGroup::Approaches),
                    7 => matches!(
                        c.chart_group,
                        ChartGroup::Departures | ChartGroup::Arrivals | ChartGroup::Approaches
                    ),
                    _ => false,
                })
                .cloned()
                .collect()
        },
    )
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
                        chart_group: match record.chart_code.as_str() {
                            "IAP" => ChartGroup::Approaches,
                            "ODP" | "DP" | "DAU" => ChartGroup::Departures,
                            "STAR" => ChartGroup::Arrivals,
                            "APD" => ChartGroup::APD,
                            "MIN" | "LAH" | "HOT" => ChartGroup::General,
                            _ => ChartGroup::General,
                        },
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
