// Copyright (c) IAV  GmbH.
// Licensed under the MIT license.
// SPDX-License-Identifier: MIT

use interfaces::chariott::service_discovery::core::v1::service_registry_client::ServiceRegistryClient;
use interfaces::chariott::service_discovery::core::v1::DiscoverRequest;
use interfaces::invehicle_digital_twin::v1::invehicle_digital_twin_client::InvehicleDigitalTwinClient;
use interfaces::invehicle_digital_twin::v1::{EndpointInfo, FindByIdRequest};
use log::{debug, info};
use tonic::{Request, Status};

/// Use Chariott Service Discovery to discover a service.
///
/// # Arguments
/// * `chariott_uri` - Chariott's URI.
/// * `namespace` - The service's namespace.
/// * `name` - The service's name.
/// * `version` - The service's version.
/// # `communication_kind` - The service's communication kind.
/// # `communication_reference` - The service's communication reference.
pub async fn discover_service_using_chariott(
    chariott_uri: &str,
    namespace: &str,
    name: &str,
    version: &str,
    communication_kind: &str,
    communication_reference: &str,
) -> Result<String, Status> {
    let mut client = ServiceRegistryClient::connect(chariott_uri.to_string())
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let request = Request::new(DiscoverRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: version.to_string(),
    });

    let response = client
        .discover(request)
        .await
        .map_err(|error| Status::internal(error.to_string()))?;

    let service = response.into_inner().service.ok_or_else(|| Status::not_found("Did not find a service in Chariott with namespace '{namespace}', name '{name}' and version {version}"))?;

    if service.communication_kind != communication_kind
        && service.communication_reference != communication_reference
    {
        return Err(Status::not_found(
            "Did not find a service in Chariott with namespace '{namespace}', name '{name}' and version {version} that has communication kind '{communication_kind} and communication_reference '{communication_reference}''",
        ));
    }

    Ok(service.uri)
}

/// If the 'containerize' feature is set, this function will modify the localhost URI to point to
/// the container's localhost DNS alias. Otherwise, returns the URI as a string.
///
/// # Arguments
/// * `uri` - The uri to potentially modify.
pub fn get_uri(uri: &str) -> Result<String, Status> {
    #[cfg(feature = "containerize")]
    let uri = {
        // Container env variable names.
        let host_gateway_env_var: &str = "HOST_GATEWAY";
        let host_alias_env_var: &str = "LOCALHOST_ALIAS";

        // Return an error if container env variables are not set.
        let host_gateway = std::env::var(host_gateway_env_var).map_err(|err| {
            Status::failed_precondition(format!(
                "Unable to get environment var '{host_gateway_env_var}' with error: {err}"
            ))
        })?;
        let host_alias = std::env::var(host_alias_env_var).map_err(|err| {
            Status::failed_precondition(format!(
                "Unable to get environment var '{host_alias_env_var}' with error: {err}"
            ))
        })?;

        uri.replace(&host_alias, &host_gateway)
    };

    Ok(uri.to_string())
}

/// Use Ibeji to discover the endpoint for a digital twin provider that satifies the requirements.
///
/// # Arguments
/// * `invehicle_digitial_twin_service_uri` - In-vehicle digital twin service URI.
/// * `entity_id` - The matching entity id.
/// * `protocol` - The required protocol.
/// * `operations` - The required operations.
pub async fn discover_digital_twin_provider_using_ibeji(
    invehicle_digitial_twin_service_uri: &str,
    entity_id: &str,
    protocol: &str,
    operations: &[String],
) -> Result<EndpointInfo, String> {
    info!("Sending a find_by_id request for entity id {entity_id} to the In-Vehicle Digital Twin Service URI {invehicle_digitial_twin_service_uri}");

    let mut client =
        InvehicleDigitalTwinClient::connect(invehicle_digitial_twin_service_uri.to_string())
            .await
            .map_err(|error| format!("{error}"))?;
    let request = tonic::Request::new(FindByIdRequest {
        id: entity_id.to_string(),
    });
    let response = client
        .find_by_id(request)
        .await
        .map_err(|error| error.to_string())?;
    let response_inner = response.into_inner();
    debug!("Received the response for the find_by_id request");
    info!("response_payload: {:?}", response_inner.entity_access_info);

    match response_inner
        .entity_access_info
        .ok_or_else(|| "Did not find the entity".to_string())?
        .endpoint_info_list
        .iter()
        .find(|endpoint_info| {
            endpoint_info.protocol == protocol
                && is_subset(operations, endpoint_info.operations.as_slice())
        })
        .cloned()
    {
        Some(mut result) => {
            info!(
                "Found a matching endpoint for entity id {entity_id} that has URI {}",
                result.uri
            );

            result.uri = get_uri(&result.uri)
                .map_err(|err| format!("Failed to get provider URI due to error: {err}"))?;

            Ok(result)
        }
        None => Err("Did not find an endpoint that met our requirements".to_string()),
    }
}

/// Is the provided subset a subset of the provided superset?
///
/// # Arguments
/// * `subset` - The provided subset.
/// * `superset` - The provided superset.
fn is_subset(subset: &[String], superset: &[String]) -> bool {
    subset.iter().all(|subset_member| {
        superset
            .iter()
            .any(|supserset_member| subset_member == supserset_member)
    })
}
