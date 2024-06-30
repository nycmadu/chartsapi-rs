# What is this?
This repo is a Rust clone of the AviationApi `/charts` endpoint. See docs from AviationAPI [here](https://docs.aviationapi.com/). This clone supports FAA LID and ICAO airport ids as well as the `group` query param as specified by AviationAPI.

Unlike AviationAPI, this clone does not re-host the chart PDFs. Instead, the API returns links to the FAA-hosted PDFs.
