# MoonBlokz Telemetry HUB Specification

moonblokz-telemetry-hub is a cloud based central system to collect telemetry data from MoonBlokz nodes and transfer it to the MoonBlokz log collector. This component is running in the cloud and accepts communications (via HTTPS requests) both from the moonblokz-probes, the moonblokz-log-collector and moonblokz cli.

Technically it is WASM (WASIP2) application written in Rust, using Spin SDK and deployed to the Fermyon cloud service.

Variables:
moonblokz-telemetry-hub uses the following variables defined in spin.toml:

- probe_api_key: an API key, that all probes must send in the header of all requests (shared between all nodes)
- log_collector_api_key: an API key, that the log_collector must send in the header of all requests.
- cli_api_key: an API key, that the moonblokz_cli must send in the header of all requests.
- delete_timeout: the timeout to automatically delay log messages (minutes)
- default_upload_interval: default upload interval from nodes

Data storage:
- The application uses both sqlite and key-value storage
- In sqlite the app use the following tables: 
  - log_messages
    - columns:
      - id: autoincrement
      - timestamp
      - node: the node that sent the log message (u32)
      - log message
  - commands
    - columns:
      - id: autoincrement
      - timestamp
      - node_id: node identifier (u32)
      - command string

- In key value storage we store the following values:
    - the last probe update interval (we need this to calculate delay)
    - timestamp of the last delete command


Working modell:
- at start: max_upload_interval=default_upload_interval

moonblokz-telemetry-hub is HTTP triggered application. It handles the following types of requests:
- I. Update from the probes:
  - URL path: /update
  - headers: 
    - `X-Node-ID: <node_id>`  
    - `X-Api-Key: <api_key>`  
  - method: POST
  - Payload example: 
    ```json
    {
      "logs": [
        {
          "timestamp": "2023-10-27T10:00:00Z",
          "message": "[INFO] Node initialized."
        },
        {
          "timestamp": "2023-10-27T10:00:05Z",
          "message": "[DEBUG] Packet received from peer."
        }
      ]
    }
  - process: 
    - check api_key
    - save every log item to database
    - if current_time-last delete time>5 minutes delete all log and command items from database where time_stamp<current_time-delete_timeout(variable)
    - check the waiting commands for this node and returns it in the response in json format and deletes them (return it in json array).
- II. Log download by log collector:
  - URL path: /download
  - query: last_log_message_id: the identifier of the last already received log item.
  - headers: 
    - `X-Api-Key: <api_key>`  
  - method: GET
  - Response payload example: 
    ```json
    {
      "logs": [
        {
          "item_id":1,
          "timestamp": "2023-10-27T10:00:00Z",
          "node_id":21,
          "message": "[INFO] Node initialized."
        },
        {
          "item_id":2,
          "timestamp": "2023-10-27T10:00:05Z",
          "node_id":22,
          "message": "[DEBUG] Packet received from peer."
        }
      ]
    }
  - process: 
    - check api_key
    - get the log items where log_message.identifier>last_log_message_id and log_message.timestamp<current_time-max_upload_interval*1.1 (the multiplier is here to calculate with network delays)
    - return the log messages in json format
    
- III. Send command by the CLI client:
  - URL path: /command
  - headers: 
    - `X-Api-Key: <api_key>`  
  - method: POST
  - Request payload examples are in the cli specification
  - Response codes: 200-success, 4xx - Bad request, 5xx-internal error
    
  - process: 
    - check api_key
    - check if node_id is specified
      - if it is specified save the command to the database with the given node_id
      - if it is not specified, query all distinct node_id from the log_messages and save the command separately for every node_id
      - if command is a set_update_interval command:
        - if node_id is specified calculate the maximum of current_max_upload_interval and the new interval
        - if node_id is not specified current_max_upload_interval=new interval
      - return the result
    


