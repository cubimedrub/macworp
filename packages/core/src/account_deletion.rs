/// What happens to a user's projects, workflows and groups when their account is deleted, in
/// plain words. Shown to users before they confirm deleting an account, and mirrored by the
/// "Deleting an account" section of the README; change all of them together with
/// `authorization::Departure` in the backend.
pub const RULES: [&str; 6] = [
    "Your shares and group memberships are removed.",
    "Projects and workflows that another user or group also owns stay; that owner takes over.",
    "A project you solely owned stays without an owner while somebody can still edit it, otherwise it is deleted with its files. Viewers alone do not keep it.",
    "A workflow you solely owned stays without an owner while somebody can still view or edit it, otherwise it is deleted.",
    "Groups you own stay without an owner while a share uses them and they have members, otherwise they are deleted. Groups left without members are deleted too, together with their shares.",
    "Runs you started are kept without a starter. Resources without an owner are managed by administrators.",
];
