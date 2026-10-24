//! Deterministic work accounting for the streaming maintenance contract (§3.5/I7).
//! Counts are cumulative; snapshot serialization and external observer traversal
//! are separate from work performed while producing an Update.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceWork {
    /// Source checks plus ordered statement-budget summary visits.
    pub statements_checked: u64,
    pub instances_examined: u64,
    pub resource_ranges_examined: u64,
    pub wire_nodes_examined: u64,
    pub props_compared: u64,
    pub diagnostics_examined: u64,
}

impl std::ops::AddAssign for MaintenanceWork {
    fn add_assign(&mut self, other: Self) {
        self.statements_checked = self
            .statements_checked
            .saturating_add(other.statements_checked);
        self.instances_examined = self
            .instances_examined
            .saturating_add(other.instances_examined);
        self.resource_ranges_examined = self
            .resource_ranges_examined
            .saturating_add(other.resource_ranges_examined);
        self.wire_nodes_examined = self
            .wire_nodes_examined
            .saturating_add(other.wire_nodes_examined);
        self.props_compared = self.props_compared.saturating_add(other.props_compared);
        self.diagnostics_examined = self
            .diagnostics_examined
            .saturating_add(other.diagnostics_examined);
    }
}
