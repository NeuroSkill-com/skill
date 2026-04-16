### Features

- Add device filter to EEG embedding search. A dropdown in the search controls lets users filter by device (e.g. MuseS-F921, AttentivU-053) or search across all devices. Backed by `GET /v1/search/devices` and `device_name` parameter on the streaming search endpoint.
