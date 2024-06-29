use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupedChartsDto {
    #[serde(rename = "General", skip_serializing_if = "Option::is_none")]
    pub general: Option<Vec<ChartDto>>,
    #[serde(rename = "DP", skip_serializing_if = "Option::is_none")]
    pub departures: Option<Vec<ChartDto>>,
    #[serde(rename = "STAR", skip_serializing_if = "Option::is_none")]
    pub arrivals: Option<Vec<ChartDto>>,
    #[serde(rename = "CAPP", skip_serializing_if = "Option::is_none")]
    pub approaches: Option<Vec<ChartDto>>,
}

impl GroupedChartsDto {
    pub const fn new() -> Self {
        Self {
            general: None,
            departures: None,
            arrivals: None,
            approaches: None,
        }
    }

    pub fn add_general(&mut self, chart_dto: ChartDto) {
        match self.general {
            Some(ref mut charts) => charts.push(chart_dto),
            None => self.general = Some(vec![chart_dto]),
        }
    }

    pub fn add_departure(&mut self, chart_dto: ChartDto) {
        match self.departures {
            Some(ref mut charts) => charts.push(chart_dto),
            None => self.departures = Some(vec![chart_dto]),
        }
    }

    pub fn add_arrival(&mut self, chart_dto: ChartDto) {
        match self.arrivals {
            Some(ref mut charts) => charts.push(chart_dto),
            None => self.arrivals = Some(vec![chart_dto]),
        }
    }

    pub fn add_approach(&mut self, chart_dto: ChartDto) {
        match self.approaches {
            Some(ref mut charts) => charts.push(chart_dto),
            None => self.approaches = Some(vec![chart_dto]),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseDto {
    Charts(Vec<ChartDto>),
    GroupedCharts(GroupedChartsDto),
}
