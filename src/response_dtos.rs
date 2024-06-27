use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChartGroup {
    General,
    Departures,
    Arrivals,
    Approaches,
    APD,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartDto {
    pub state: String,
    pub state_full: String,
    pub city: String,
    pub volume: String,
    pub airport_name: String,
    pub military: String,
    pub faa_ident: String,
    pub icao_ident: String,
    pub chart_seq: String,
    pub chart_code: String,
    pub chart_name: String,
    pub pdf_name: String,
    pub pdf_path: String,
    #[serde(skip_serializing)]
    pub chart_group: ChartGroup,
}
