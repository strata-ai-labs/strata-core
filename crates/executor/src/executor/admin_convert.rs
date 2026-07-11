use super::{
    commit_receipt, create_effect, delete_effect, output_vector_metric,
    EngineAdminCapabilitySummary, EngineAdminConfigSummary, EngineAdminDatabaseInfo,
    EngineAdminDescribeSummary, EngineAdminGraphSummary, EngineAdminHealthStatus,
    EngineAdminHealthSummary, EngineAdminMetricsSummary, EngineAdminPrimitiveSummary,
    EngineAdminVectorCollectionSummary, EngineControlHealthStatus, EngineDatabaseOpenTarget,
    EngineSpaceCreateOutcome, EngineSpaceDeleteOutcome, Output, OutputAdminCapabilities,
    OutputAdminConfig, OutputAdminControlStatus, OutputAdminDatabaseInfo, OutputAdminDescribe,
    OutputAdminGraph, OutputAdminHealth, OutputAdminHealthStatus, OutputAdminMetrics,
    OutputAdminOpenTarget, OutputAdminPrimitives, OutputAdminVectorCollection,
};

#[cfg(not(feature = "arrow"))]
use super::ExecutorError;

#[cfg(not(feature = "arrow"))]
pub(super) fn arrow_feature_disabled() -> ExecutorError {
    ExecutorError::invalid_input(
        "invalid_argument.executor.arrow_feature_disabled",
        "Arrow import/export requires the executor arrow feature",
    )
}

pub(super) const fn output_admin_open_target(
    target: EngineDatabaseOpenTarget,
) -> OutputAdminOpenTarget {
    match target {
        EngineDatabaseOpenTarget::Cache => OutputAdminOpenTarget::Cache,
        EngineDatabaseOpenTarget::DurableLocal => OutputAdminOpenTarget::DurableLocal,
    }
}

pub(super) const fn output_admin_health_status(
    status: EngineAdminHealthStatus,
) -> OutputAdminHealthStatus {
    match status {
        EngineAdminHealthStatus::Healthy => OutputAdminHealthStatus::Healthy,
        EngineAdminHealthStatus::Degraded => OutputAdminHealthStatus::Degraded,
        EngineAdminHealthStatus::Unhealthy => OutputAdminHealthStatus::Unhealthy,
    }
}

pub(super) const fn output_admin_control_status(
    status: EngineControlHealthStatus,
) -> OutputAdminControlStatus {
    match status {
        EngineControlHealthStatus::Healthy => OutputAdminControlStatus::Healthy,
        EngineControlHealthStatus::Missing => OutputAdminControlStatus::Missing,
        EngineControlHealthStatus::Corrupt => OutputAdminControlStatus::Corrupt,
        EngineControlHealthStatus::Unavailable => OutputAdminControlStatus::Unavailable,
    }
}

pub(super) fn output_admin_info(info: &EngineAdminDatabaseInfo) -> OutputAdminDatabaseInfo {
    OutputAdminDatabaseInfo {
        version: info.version.clone(),
        target: output_admin_open_target(info.target),
        created: info.created,
        durable: info.durable,
        default_branch: info.default_branch.as_str().to_owned(),
        branch_count: info.branch_count,
        space_count: info.space_count,
        open: info.open,
    }
}

pub(super) fn output_admin_health(health: &EngineAdminHealthSummary) -> OutputAdminHealth {
    OutputAdminHealth {
        status: output_admin_health_status(health.status),
        identity: output_admin_control_status(health.identity),
        registry: output_admin_control_status(health.registry),
        branch_catalog: output_admin_control_status(health.branch_catalog),
        space_catalog: health.space_catalog.map(output_admin_control_status),
        default_branch: health.default_branch.as_str().to_owned(),
        branch_count: health.branch_count,
    }
}

pub(super) fn output_admin_metrics(metrics: &EngineAdminMetricsSummary) -> OutputAdminMetrics {
    OutputAdminMetrics {
        target: output_admin_open_target(metrics.target),
        durable: metrics.durable,
        open: metrics.open,
        branch_count: metrics.branch_count,
        space_count: metrics.space_count,
        control_status: output_admin_health_status(metrics.control_status),
    }
}

pub(super) fn output_admin_config(config: &EngineAdminConfigSummary) -> OutputAdminConfig {
    OutputAdminConfig {
        target: output_admin_open_target(config.target),
        created: config.created,
        durable: config.durable,
        default_branch: config.default_branch.as_str().to_owned(),
    }
}

pub(super) fn output_admin_capabilities(
    capabilities: &EngineAdminCapabilitySummary,
) -> OutputAdminCapabilities {
    OutputAdminCapabilities {
        kv: capabilities.kv,
        json: capabilities.json,
        event: capabilities.event,
        vector: capabilities.vector,
        vector_index: capabilities.vector_index,
        graph_core: capabilities.graph_core,
        arrow: cfg!(feature = "arrow"),
        inference: cfg!(feature = "inference"),
    }
}

pub(super) fn output_admin_vector_collection(
    collection: &EngineAdminVectorCollectionSummary,
) -> OutputAdminVectorCollection {
    OutputAdminVectorCollection {
        name: collection.name.clone(),
        dimension: collection.dimension,
        metric: output_vector_metric(collection.metric),
        count: collection.count,
    }
}

pub(super) fn output_admin_graph(graph: &EngineAdminGraphSummary) -> OutputAdminGraph {
    OutputAdminGraph {
        name: graph.name.clone(),
        node_count: graph.node_count,
        edge_count: graph.edge_count,
    }
}

pub(super) fn output_admin_primitives(
    primitives: &EngineAdminPrimitiveSummary,
) -> OutputAdminPrimitives {
    OutputAdminPrimitives {
        kv_count: primitives.kv_count,
        json_count: primitives.json_count,
        event_count: primitives.event_count,
        vector_collections: primitives
            .vector_collections
            .iter()
            .map(output_admin_vector_collection)
            .collect(),
        graphs: primitives.graphs.iter().map(output_admin_graph).collect(),
    }
}

pub(super) fn output_admin_describe(describe: &EngineAdminDescribeSummary) -> OutputAdminDescribe {
    OutputAdminDescribe {
        version: describe.version.clone(),
        target: output_admin_open_target(describe.target),
        default_branch: describe.default_branch.as_str().to_owned(),
        branch: describe.branch.as_str().to_owned(),
        branches: describe
            .branches
            .iter()
            .map(|branch| branch.as_str().to_owned())
            .collect(),
        spaces: describe
            .spaces
            .iter()
            .map(|space| space.as_str().to_owned())
            .collect(),
        primitives: output_admin_primitives(&describe.primitives),
        config: output_admin_config(&describe.config),
        capabilities: output_admin_capabilities(&describe.capabilities),
    }
}

pub(super) fn output_space_create(outcome: &EngineSpaceCreateOutcome) -> Output {
    Output::SpaceCreateResult {
        space: outcome.space().as_str().to_owned(),
        effect: create_effect(outcome.created()),
        commit: outcome.commit().map(commit_receipt),
    }
}

pub(super) fn output_space_delete(outcome: &EngineSpaceDeleteOutcome) -> Output {
    Output::SpaceDeleteResult {
        space: outcome.space().as_str().to_owned(),
        force: outcome.force(),
        deleted_rows: outcome.deleted_rows(),
        effect: delete_effect(outcome.deleted()),
        commit: outcome.commit().map(commit_receipt),
    }
}
