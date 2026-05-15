use super::{
    decode_inventory, inventory_object, require_capability, validate_inventory_identity,
    QuarantineService, QuarantineServiceError, QuarantineServiceResult,
};
use crate::backend::{Backend, BackendCapability, BackendError, BackendErrorKind};
use crate::format::quarantine::QuarantineInventory;
use crate::format::FormatError;
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::ObjectName;
use std::collections::BTreeMap;
use strata_core_next::BranchId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineRecoveryClass {
    Healthy,
    PolicyDowngraded,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineReconciliationKind {
    CleanEmpty,
    CleanInventory,
    CorruptInventory,
    UnlistedQuarantineObject,
    MissingQuarantineObject,
    MalformedListedObject,
    BackendUnavailable,
}

impl QuarantineReconciliationKind {
    const fn recovery_class(self) -> QuarantineRecoveryClass {
        match self {
            Self::CleanEmpty | Self::CleanInventory => QuarantineRecoveryClass::Healthy,
            Self::CorruptInventory
            | Self::UnlistedQuarantineObject
            | Self::MissingQuarantineObject
            | Self::MalformedListedObject => QuarantineRecoveryClass::PolicyDowngraded,
            Self::BackendUnavailable => QuarantineRecoveryClass::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineBackendOperation {
    ReadInventory,
    ListBranch,
    ListFamily,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineBackendUnavailable {
    operation: QuarantineBackendOperation,
    object: Option<ObjectName>,
    source: BackendError,
}

impl QuarantineBackendUnavailable {
    fn new(
        operation: QuarantineBackendOperation,
        object: Option<ObjectName>,
        source: BackendError,
    ) -> Self {
        Self {
            operation,
            object,
            source,
        }
    }

    pub(crate) const fn operation(&self) -> QuarantineBackendOperation {
        self.operation
    }

    pub(crate) const fn object(&self) -> Option<&ObjectName> {
        self.object.as_ref()
    }

    pub(crate) const fn source(&self) -> &BackendError {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineInventoryCorruption {
    Decode(FormatError),
    DatabaseMismatch {
        expected: [u8; 16],
        actual: [u8; 16],
    },
    BranchMismatch {
        expected: BranchId,
        actual: BranchId,
    },
    CodecMismatch {
        expected: String,
        actual: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineCorruptInventory {
    object: ObjectName,
    source: QuarantineInventoryCorruption,
}

impl QuarantineCorruptInventory {
    fn new(object: ObjectName, source: QuarantineInventoryCorruption) -> Self {
        Self { object, source }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn source(&self) -> &QuarantineInventoryCorruption {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineListedObject {
    object_id: String,
    object: ObjectName,
    source_object: ObjectName,
    byte_count: u64,
}

impl QuarantineListedObject {
    fn new(
        object_id: String,
        object: ObjectName,
        source_object: ObjectName,
        byte_count: u64,
    ) -> Self {
        Self {
            object_id,
            object,
            source_object,
            byte_count,
        }
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn source_object(&self) -> &ObjectName {
        &self.source_object
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineMissingObject {
    object_id: String,
    object: ObjectName,
    source_object: ObjectName,
}

impl QuarantineMissingObject {
    fn new(object_id: String, object: ObjectName, source_object: ObjectName) -> Self {
        Self {
            object_id,
            object,
            source_object,
        }
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn source_object(&self) -> &ObjectName {
        &self.source_object
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineUnlistedObject {
    object_id: String,
    object: ObjectName,
}

impl QuarantineUnlistedObject {
    fn new(object_id: String, object: ObjectName) -> Self {
        Self { object_id, object }
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MalformedQuarantineObjectReason {
    Branch,
    ObjectId,
    Shape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineMalformedObject {
    object: ObjectName,
    branch_id: Option<BranchId>,
    object_id: Option<String>,
    reason: MalformedQuarantineObjectReason,
}

impl QuarantineMalformedObject {
    fn new(
        object: ObjectName,
        branch_id: Option<BranchId>,
        object_id: Option<String>,
        reason: MalformedQuarantineObjectReason,
    ) -> Self {
        Self {
            object,
            branch_id,
            object_id,
            reason,
        }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn branch_id(&self) -> Option<BranchId> {
        self.branch_id
    }

    pub(crate) fn object_id(&self) -> Option<&str> {
        self.object_id.as_deref()
    }

    pub(crate) const fn reason(&self) -> MalformedQuarantineObjectReason {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineReconciliationReport {
    branch_id: BranchId,
    inventory_object: ObjectName,
    inventory_present: bool,
    kind: QuarantineReconciliationKind,
    listed_objects: Vec<QuarantineListedObject>,
    missing_objects: Vec<QuarantineMissingObject>,
    unlisted_objects: Vec<QuarantineUnlistedObject>,
    malformed_objects: Vec<QuarantineMalformedObject>,
    corrupt_inventory: Option<QuarantineCorruptInventory>,
    unavailable: Option<QuarantineBackendUnavailable>,
}

impl QuarantineReconciliationReport {
    fn new(branch_id: BranchId, inventory_object: ObjectName) -> Self {
        Self {
            branch_id,
            inventory_object,
            inventory_present: false,
            kind: QuarantineReconciliationKind::CleanEmpty,
            listed_objects: Vec::new(),
            missing_objects: Vec::new(),
            unlisted_objects: Vec::new(),
            malformed_objects: Vec::new(),
            corrupt_inventory: None,
            unavailable: None,
        }
    }

    fn finish(mut self) -> Self {
        self.kind = classify_branch_report(&self);
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn inventory_object(&self) -> &ObjectName {
        &self.inventory_object
    }

    pub(crate) const fn inventory_present(&self) -> bool {
        self.inventory_present
    }

    pub(crate) const fn kind(&self) -> QuarantineReconciliationKind {
        self.kind
    }

    pub(crate) const fn recovery_class(&self) -> QuarantineRecoveryClass {
        self.kind.recovery_class()
    }

    pub(crate) fn listed_objects(&self) -> &[QuarantineListedObject] {
        &self.listed_objects
    }

    pub(crate) fn missing_objects(&self) -> &[QuarantineMissingObject] {
        &self.missing_objects
    }

    pub(crate) fn unlisted_objects(&self) -> &[QuarantineUnlistedObject] {
        &self.unlisted_objects
    }

    pub(crate) fn malformed_objects(&self) -> &[QuarantineMalformedObject] {
        &self.malformed_objects
    }

    pub(crate) const fn corrupt_inventory(&self) -> Option<&QuarantineCorruptInventory> {
        self.corrupt_inventory.as_ref()
    }

    pub(crate) const fn unavailable(&self) -> Option<&QuarantineBackendUnavailable> {
        self.unavailable.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineFamilyReconciliation {
    branch_reports: Vec<QuarantineReconciliationReport>,
    malformed_objects: Vec<QuarantineMalformedObject>,
    unavailable: Option<QuarantineBackendUnavailable>,
}

impl QuarantineFamilyReconciliation {
    fn new(
        branch_reports: Vec<QuarantineReconciliationReport>,
        malformed_objects: Vec<QuarantineMalformedObject>,
        unavailable: Option<QuarantineBackendUnavailable>,
    ) -> Self {
        Self {
            branch_reports,
            malformed_objects,
            unavailable,
        }
    }

    pub(crate) fn branch_reports(&self) -> &[QuarantineReconciliationReport] {
        &self.branch_reports
    }

    pub(crate) fn malformed_objects(&self) -> &[QuarantineMalformedObject] {
        &self.malformed_objects
    }

    pub(crate) const fn unavailable(&self) -> Option<&QuarantineBackendUnavailable> {
        self.unavailable.as_ref()
    }

    pub(crate) fn kind(&self) -> QuarantineReconciliationKind {
        if self.unavailable.is_some()
            || self
                .branch_reports
                .iter()
                .any(|report| report.recovery_class() == QuarantineRecoveryClass::Unavailable)
        {
            return QuarantineReconciliationKind::BackendUnavailable;
        }
        if !self.malformed_objects.is_empty() {
            if let Some(kind) = highest_policy_kind(
                Some(QuarantineReconciliationKind::MalformedListedObject),
                self.branch_reports
                    .iter()
                    .map(QuarantineReconciliationReport::kind),
            ) {
                return kind;
            }
        }
        highest_policy_kind(
            None,
            self.branch_reports
                .iter()
                .map(QuarantineReconciliationReport::kind),
        )
        .unwrap_or(if self.branch_reports.is_empty() {
            QuarantineReconciliationKind::CleanEmpty
        } else {
            QuarantineReconciliationKind::CleanInventory
        })
    }

    pub(crate) fn recovery_class(&self) -> QuarantineRecoveryClass {
        self.kind().recovery_class()
    }
}

impl QuarantineService<'_> {
    pub(crate) fn reconcile_branch_quarantine(
        &self,
        branch_id: BranchId,
        expected_database_id: [u8; 16],
        expected_codec_id: &str,
    ) -> QuarantineServiceResult<QuarantineReconciliationReport> {
        require_capability(self.backend, BackendCapability::ListPrefix)?;
        require_capability(self.backend, BackendCapability::ReadObject)?;

        let inventory_object = inventory_object(branch_id)?;
        let mut report = QuarantineReconciliationReport::new(branch_id, inventory_object.clone());
        // Missing inventory is healthy only after the branch quarantine prefix
        // has been inspected. An orphaned quarantine object means recovery
        // should retain and report policy-downgraded state, not synthesize an
        // empty inventory.
        let listing = match list_branch_quarantine_objects(self.backend, branch_id) {
            Ok(listing) => listing,
            Err(source) => {
                report.unavailable = Some(QuarantineBackendUnavailable::new(
                    QuarantineBackendOperation::ListBranch,
                    None,
                    source,
                ));
                return Ok(report.finish());
            }
        };
        report.malformed_objects = listing.malformed_objects;

        let Some(inventory) = (match load_reconciliation_inventory(
            self.backend,
            branch_id,
            &inventory_object,
            expected_database_id,
            expected_codec_id,
        )? {
            InventoryReconciliationLoad::Present(inventory) => {
                report.inventory_present = true;
                Some(inventory)
            }
            InventoryReconciliationLoad::Absent => None,
            InventoryReconciliationLoad::Corrupt(corrupt) => {
                // Corrupt inventory dominates the branch classification, but
                // visible object facts found during the prefix scan stay
                // attached to the report for diagnostics. Recovery should not
                // lose orphaned bytes just because the inventory cannot decode.
                report.inventory_present = true;
                report.corrupt_inventory = Some(corrupt);
                report.unlisted_objects = listing
                    .objects
                    .into_iter()
                    .map(|(object_id, object)| QuarantineUnlistedObject::new(object_id, object))
                    .collect();
                return Ok(report.finish());
            }
            InventoryReconciliationLoad::Unavailable(unavailable) => {
                report.unavailable = Some(unavailable);
                return Ok(report.finish());
            }
        }) else {
            report.unlisted_objects = listing
                .objects
                .into_iter()
                .map(|(object_id, object)| QuarantineUnlistedObject::new(object_id, object))
                .collect();
            return Ok(report.finish());
        };

        let mut remaining_objects = listing.objects;
        for entry in inventory.entries() {
            let object = quarantine_object_name(branch_id, entry.object_id())?;
            if remaining_objects.remove(entry.object_id()).is_some() {
                report.listed_objects.push(QuarantineListedObject::new(
                    entry.object_id().to_owned(),
                    object,
                    entry.source_object().clone(),
                    entry.byte_count(),
                ));
            } else {
                report.missing_objects.push(QuarantineMissingObject::new(
                    entry.object_id().to_owned(),
                    object,
                    entry.source_object().clone(),
                ));
            }
        }
        report.unlisted_objects = remaining_objects
            .into_iter()
            .map(|(object_id, object)| QuarantineUnlistedObject::new(object_id, object))
            .collect();

        Ok(report.finish())
    }

    pub(crate) fn reconcile_quarantine_family(
        &self,
        expected_database_id: [u8; 16],
        expected_codec_id: &str,
    ) -> QuarantineServiceResult<QuarantineFamilyReconciliation> {
        require_capability(self.backend, BackendCapability::ListPrefix)?;

        let prefix = ObjectLayout::quarantine_prefix()
            .map_err(|source| QuarantineServiceError::Layout { source })?;
        let objects = match self.backend.list_prefix(&prefix) {
            Ok(objects) => objects,
            Err(source) => {
                return Ok(QuarantineFamilyReconciliation::new(
                    Vec::new(),
                    Vec::new(),
                    Some(QuarantineBackendUnavailable::new(
                        QuarantineBackendOperation::ListFamily,
                        None,
                        source,
                    )),
                ));
            }
        };

        let mut branch_ids = BTreeMap::new();
        let mut malformed_objects = Vec::new();
        for object in objects {
            match parse_quarantine_object(object) {
                ParsedQuarantineObject::Ignored => {}
                ParsedQuarantineObject::Inventory { branch_id }
                | ParsedQuarantineObject::Object { branch_id, .. } => {
                    branch_ids.insert(branch_id.to_string(), branch_id);
                }
                ParsedQuarantineObject::Malformed(malformed) => {
                    // Malformed object ids with a valid branch can be
                    // classified by the branch-local report. Family-level
                    // malformed facts are reserved for names a branch-local
                    // prefix cannot discover, such as invalid branch text.
                    match (malformed.branch_id(), malformed.reason()) {
                        (Some(branch_id), MalformedQuarantineObjectReason::ObjectId) => {
                            branch_ids.insert(branch_id.to_string(), branch_id);
                        }
                        _ => malformed_objects.push(malformed),
                    }
                }
            }
        }

        let mut branch_reports = Vec::new();
        for branch_id in branch_ids.into_values() {
            branch_reports.push(self.reconcile_branch_quarantine(
                branch_id,
                expected_database_id,
                expected_codec_id,
            )?);
        }
        Ok(QuarantineFamilyReconciliation::new(
            branch_reports,
            malformed_objects,
            None,
        ))
    }
}

#[derive(Debug)]
struct BranchQuarantineListing {
    objects: BTreeMap<String, ObjectName>,
    malformed_objects: Vec<QuarantineMalformedObject>,
}

enum InventoryReconciliationLoad {
    Present(QuarantineInventory),
    Absent,
    Corrupt(QuarantineCorruptInventory),
    Unavailable(QuarantineBackendUnavailable),
}

enum ParsedQuarantineObject {
    Inventory {
        branch_id: BranchId,
    },
    Object {
        branch_id: BranchId,
        object_id: String,
        object: ObjectName,
    },
    Malformed(QuarantineMalformedObject),
    Ignored,
}

fn list_branch_quarantine_objects(
    backend: &dyn Backend,
    branch_id: BranchId,
) -> Result<BranchQuarantineListing, BackendError> {
    let prefix = ObjectLayout::branch_quarantine_prefix(&branch_id.to_string())
        .map_err(|_| BackendError::new(BackendErrorKind::InvalidObjectName, "invalid prefix"))?;
    let mut objects = BTreeMap::new();
    let mut malformed_objects = Vec::new();

    for object in backend.list_prefix(&prefix)? {
        match parse_quarantine_object(object) {
            ParsedQuarantineObject::Object {
                branch_id: listed_branch_id,
                object_id,
                object,
            } if listed_branch_id == branch_id => {
                objects.insert(object_id, object);
            }
            ParsedQuarantineObject::Inventory {
                branch_id: listed_branch_id,
            } if listed_branch_id == branch_id => {}
            ParsedQuarantineObject::Malformed(malformed)
                if malformed.branch_id() == Some(branch_id) =>
            {
                malformed_objects.push(malformed);
            }
            _ => {}
        }
    }

    Ok(BranchQuarantineListing {
        objects,
        malformed_objects,
    })
}

fn load_reconciliation_inventory(
    backend: &dyn Backend,
    branch_id: BranchId,
    object: &ObjectName,
    expected_database_id: [u8; 16],
    expected_codec_id: &str,
) -> QuarantineServiceResult<InventoryReconciliationLoad> {
    let bytes = match backend.read_object(object) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == BackendErrorKind::NotFound => {
            return Ok(InventoryReconciliationLoad::Absent);
        }
        Err(source) => {
            return Ok(InventoryReconciliationLoad::Unavailable(
                QuarantineBackendUnavailable::new(
                    QuarantineBackendOperation::ReadInventory,
                    Some(object.clone()),
                    source,
                ),
            ));
        }
    };

    let inventory = match decode_inventory(object, &bytes) {
        Ok(inventory) => inventory,
        Err(QuarantineServiceError::Decode { source, .. }) => {
            return Ok(InventoryReconciliationLoad::Corrupt(
                QuarantineCorruptInventory::new(
                    object.clone(),
                    QuarantineInventoryCorruption::Decode(source),
                ),
            ));
        }
        Err(source) => return Err(source),
    };

    // Identity mismatches are recovery facts rather than ordinary service
    // errors here. The bytes exist, but they cannot safely describe this
    // database/branch/codec tuple.
    match validate_inventory_identity(
        object,
        branch_id,
        expected_database_id,
        expected_codec_id,
        inventory,
    ) {
        Ok(inventory) => Ok(InventoryReconciliationLoad::Present(inventory)),
        Err(source) => Ok(InventoryReconciliationLoad::Corrupt(
            QuarantineCorruptInventory::new(
                object.clone(),
                corruption_from_identity_error(source)?,
            ),
        )),
    }
}

fn corruption_from_identity_error(
    source: QuarantineServiceError,
) -> QuarantineServiceResult<QuarantineInventoryCorruption> {
    match source {
        QuarantineServiceError::DatabaseMismatch {
            expected, actual, ..
        } => Ok(QuarantineInventoryCorruption::DatabaseMismatch { expected, actual }),
        QuarantineServiceError::BranchMismatch {
            expected, actual, ..
        } => Ok(QuarantineInventoryCorruption::BranchMismatch { expected, actual }),
        QuarantineServiceError::CodecMismatch {
            expected, actual, ..
        } => Ok(QuarantineInventoryCorruption::CodecMismatch { expected, actual }),
        source => Err(source),
    }
}

fn classify_branch_report(report: &QuarantineReconciliationReport) -> QuarantineReconciliationKind {
    if report.unavailable.is_some() {
        return QuarantineReconciliationKind::BackendUnavailable;
    }
    if report.corrupt_inventory.is_some() {
        return QuarantineReconciliationKind::CorruptInventory;
    }
    if !report.malformed_objects.is_empty() {
        return QuarantineReconciliationKind::MalformedListedObject;
    }
    if !report.missing_objects.is_empty() {
        return QuarantineReconciliationKind::MissingQuarantineObject;
    }
    if !report.unlisted_objects.is_empty() {
        return QuarantineReconciliationKind::UnlistedQuarantineObject;
    }
    if report.inventory_present {
        QuarantineReconciliationKind::CleanInventory
    } else {
        QuarantineReconciliationKind::CleanEmpty
    }
}

fn highest_policy_kind<I>(
    initial: Option<QuarantineReconciliationKind>,
    candidates: I,
) -> Option<QuarantineReconciliationKind>
where
    I: IntoIterator<Item = QuarantineReconciliationKind>,
{
    candidates
        .into_iter()
        .filter(|kind| kind.recovery_class() == QuarantineRecoveryClass::PolicyDowngraded)
        .fold(initial, |current, candidate| match current {
            Some(existing) if policy_rank(existing) <= policy_rank(candidate) => Some(existing),
            _ => Some(candidate),
        })
}

fn policy_rank(kind: QuarantineReconciliationKind) -> u8 {
    match kind {
        QuarantineReconciliationKind::CorruptInventory => 0,
        QuarantineReconciliationKind::MalformedListedObject => 1,
        QuarantineReconciliationKind::MissingQuarantineObject => 2,
        QuarantineReconciliationKind::UnlistedQuarantineObject => 3,
        QuarantineReconciliationKind::CleanEmpty
        | QuarantineReconciliationKind::CleanInventory
        | QuarantineReconciliationKind::BackendUnavailable => 4,
    }
}

fn parse_quarantine_object(object: ObjectName) -> ParsedQuarantineObject {
    let raw = object.as_str().to_owned();
    let mut parts = raw.split('/');
    let Some(family) = parts.next() else {
        return ParsedQuarantineObject::Ignored;
    };
    if family != ObjectFamily::Quarantine.as_str() {
        return ParsedQuarantineObject::Ignored;
    }

    let Some(branch_text) = parts.next() else {
        return ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
            object,
            None,
            None,
            MalformedQuarantineObjectReason::Shape,
        ));
    };
    let branch_id = match BranchId::parse_str(branch_text) {
        Ok(branch_id) if branch_id.to_string() == branch_text => branch_id,
        _ => {
            return ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
                object,
                None,
                None,
                MalformedQuarantineObjectReason::Branch,
            ));
        }
    };

    let Some(component) = parts.next() else {
        return ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
            object,
            Some(branch_id),
            None,
            MalformedQuarantineObjectReason::Shape,
        ));
    };

    if parts.next().is_some() {
        return ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
            object,
            Some(branch_id),
            Some(component.to_owned()),
            MalformedQuarantineObjectReason::ObjectId,
        ));
    }

    let Some(inventory_id) = reserved_inventory_object_id(branch_id) else {
        return ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
            object,
            Some(branch_id),
            None,
            MalformedQuarantineObjectReason::Shape,
        ));
    };
    if component == inventory_id {
        return ParsedQuarantineObject::Inventory { branch_id };
    }

    let object_id = component.to_owned();
    match quarantine_object_name(branch_id, &object_id) {
        Ok(expected) if expected == object => ParsedQuarantineObject::Object {
            branch_id,
            object_id,
            object,
        },
        _ => ParsedQuarantineObject::Malformed(QuarantineMalformedObject::new(
            object,
            Some(branch_id),
            Some(object_id),
            MalformedQuarantineObjectReason::ObjectId,
        )),
    }
}

fn quarantine_object_name(
    branch_id: BranchId,
    object_id: &str,
) -> QuarantineServiceResult<ObjectName> {
    ObjectLayout::quarantine_object(&branch_id.to_string(), object_id)
        .map_err(|source| QuarantineServiceError::Layout { source })
}

fn reserved_inventory_object_id(branch_id: BranchId) -> Option<String> {
    let Ok(object) = ObjectLayout::quarantine_manifest(&branch_id.to_string()) else {
        return None;
    };
    object.as_str().rsplit('/').next().map(str::to_owned)
}
