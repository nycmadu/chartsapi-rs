#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use crate::faa_metafile::{DigitalTpp, ProductSet};
use crate::response_dtos::{ChartDto, ChartGroup};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use indexmap::IndexMap;
use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{debug, info, warn};

mod faa_metafile;
mod response_dtos;

struct ChartsHashMaps {
    faa: IndexMap<String, Vec<ChartDto>>,
    icao: IndexMap<String, String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Initialize current_cycle and in-memory hashmaps for FAA/ICAO id lookup
    let current_cycle = RwLock::new(fetch_current_cycle().await.unwrap_or_else(|e| {
        warn!(
            "Error initializing current cycle, falling back to default: {}",
            e
        );
        "2406".to_string()
    }));
    let hashmaps = Arc::new(RwLock::new(
        load_charts(&current_cycle.read().await).await.unwrap(),
    ));
    let axum_state = Arc::clone(&hashmaps);

    // Spawn cycle and chart update loop
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            match fetch_current_cycle().await {
                Ok(fetched_cycle) => {
                    if fetched_cycle.eq_ignore_ascii_case(&current_cycle.read().await) {
                        return;
                    }

                    info!("Found new cycle: {fetched_cycle}");
                    match load_charts(&fetched_cycle).await {
                        Ok(new_charts) => {
                            *hashmaps.write().await = new_charts;
                            *current_cycle.write().await = fetched_cycle;
                        }
                        Err(e) => warn!("Error while fetching charts: {}", e),
                    }
                }
                Err(e) => warn!("Error while fetching current cycle: {}", e),
            }
        }
    });

    // Create and run axum app
    let app = Router::new()
        .route("/v1/charts", get(charts_handler))
        .with_state(axum_state)
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
    State(hashmaps): State<Arc<RwLock<ChartsHashMaps>>>,
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
        if let Some(charts) = lookup_charts(airport, &hashmaps).await {
            results.insert(
                airport.to_owned(),
                filter_charts_group(&charts, chart_options.group),
            );
        }
    }
    (StatusCode::OK, Json(results)).into_response()
}

async fn lookup_charts(
    apt_id: &str,
    hashmaps: &Arc<RwLock<ChartsHashMaps>>,
) -> Option<Vec<ChartDto>> {
    let reader = hashmaps.read().await;
    reader.faa.get(&apt_id.to_uppercase()).map_or_else(
        || {
            reader
                .icao
                .get(&apt_id.to_uppercase())
                .and_then(|faa_id| reader.faa.get(faa_id).cloned())
        },
        |charts| Some(charts.clone()),
    )
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

async fn load_charts(current_cycle: &str) -> Result<ChartsHashMaps, anyhow::Error> {
    debug!("Starting charts metafile request");
    let base_url = cycle_url(current_cycle);
    let metafile = reqwest::get(format!("{base_url}/xml_data/d-tpp_Metafile.xml"))
        .await?
        .text()
        .await?;
    debug!("Charts metafile request completed");
    let dtpp = from_str::<DigitalTpp>(&metafile)?;
    let mut faa: IndexMap<String, Vec<ChartDto>> = IndexMap::new();
    let mut icao: IndexMap<String, String> = IndexMap::new();
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
                        pdf_path: format!("{base_url}/{pdf}", pdf = record.pdf_name),
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

                    if !chart_dto.icao_ident.is_empty() {
                        icao.insert(chart_dto.icao_ident.clone(), chart_dto.faa_ident.clone());
                    }

                    count += 1;
                }
            }
        }
    }

    info!("Loaded {num} charts", num = count);
    Ok(ChartsHashMaps { faa, icao })
}

async fn fetch_current_cycle() -> Result<String, anyhow::Error> {
    info!("Fetching current cycle");
    let cycle_xml = reqwest::get("https://external-api.faa.gov/apra/dtpp/info")
        .await?
        .text()
        .await?;
    let product_set = from_str::<ProductSet>(&cycle_xml)?;
    let date = NaiveDate::parse_from_str(&product_set.edition.date, "%m/%d/%Y")?;
    let cycle_str = date.format("%y%m").to_string();
    info!("Found current cycle: {cycle_str}");
    Ok(cycle_str)
}

fn cycle_url(current_cycle: &str) -> String {
    format!("https://aeronav.faa.gov/d-tpp/{current_cycle}",)
}
