//! The volume mapping grammar, exercised where the separator is ambiguous.
//!
//! The cases that matter are the ones a hand rolled split gets wrong while
//! still producing two non empty halves, because those survive every later
//! check and reach the container as a mount nobody asked for.

use lightshuttle_manifest::canonical::{MappingSource, VolumeMapping, VolumeMappingError};

fn host_path(mapping: &str) -> String {
    match VolumeMapping::parse(mapping)
        .expect("mapping parses")
        .source()
    {
        MappingSource::HostPath(path) => path.clone(),
        MappingSource::Named(name) => panic!("expected a host path, got named volume `{name}`"),
    }
}

fn named(mapping: &str) -> String {
    match VolumeMapping::parse(mapping)
        .expect("mapping parses")
        .source()
    {
        MappingSource::Named(name) => name.clone(),
        MappingSource::HostPath(path) => panic!("expected a named volume, got host path `{path}`"),
    }
}

#[test]
fn a_named_volume_maps_to_a_container_path() {
    let mapping = VolumeMapping::parse("pgdata:/var/lib/postgresql/data").expect("mapping parses");
    assert_eq!(named("pgdata:/var/lib/postgresql/data"), "pgdata");
    assert_eq!(mapping.target(), "/var/lib/postgresql/data");
}

#[test]
fn a_relative_host_path_is_a_host_path() {
    assert_eq!(host_path("./src:/app"), "./src");
}

#[test]
fn an_absolute_posix_host_path_is_a_host_path() {
    assert_eq!(host_path("/var/data:/data"), "/var/data");
}

#[test]
fn a_parent_relative_host_path_is_a_host_path() {
    assert_eq!(host_path("../shared:/shared"), "../shared");
}

// The defect of issue #282. Splitting on the first colon yielded a named
// volume `C` and a target of `\project\data:/data`, both non empty, so
// nothing downstream noticed.
#[test]
fn a_windows_drive_path_is_a_host_path_not_a_volume_named_c() {
    assert_eq!(host_path(r"C:\project\data:/data"), r"C:\project\data");
}

#[test]
fn a_windows_drive_path_keeps_its_full_source_and_its_target() {
    let mapping = VolumeMapping::parse(r"C:\project\data:/data").expect("mapping parses");
    assert_eq!(mapping.target(), "/data");
}

#[test]
fn a_drive_path_with_forward_slashes_is_a_host_path() {
    assert_eq!(host_path("D:/project/data:/data"), "D:/project/data");
}

#[test]
fn a_lowercase_drive_letter_is_recognised() {
    assert_eq!(host_path(r"c:\data:/data"), r"c:\data");
}

#[test]
fn a_drive_root_is_a_host_path() {
    assert_eq!(host_path(r"C:\:/data"), r"C:\");
}

// `c:/data` is two valid readings at once: a volume named `c` mounted at
// `/data`, or a drive path missing its target. A mapping must carry two
// fields, so the only complete reading wins.
#[test]
fn a_single_letter_volume_name_stays_a_named_volume() {
    assert_eq!(named("c:/data"), "c");
}

// The same ambiguity with a further colon: now the drive reading is the
// complete one.
#[test]
fn a_forward_slash_drive_path_wins_when_a_target_follows() {
    assert_eq!(host_path("c:/project:/data"), "c:/project");
}

#[test]
fn a_two_letter_prefix_is_not_a_drive_letter() {
    assert_eq!(named("ab:/data"), "ab");
}

// The defect of issue #300. The trailing field was folded into the target,
// so the container mounted at the literal path `/app:ro`.
#[test]
fn a_read_only_option_is_rejected_rather_than_folded_into_the_target() {
    let error = VolumeMapping::parse("./data:/app:ro").expect_err("a third field is rejected");
    assert!(
        matches!(&error, VolumeMappingError::UnsupportedField { field, .. } if field == "ro"),
        "expected the trailing field to be named, got {error:?}"
    );
}

#[test]
fn a_read_write_option_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse("cache:/var/cache:rw"),
        Err(VolumeMappingError::UnsupportedField { .. })
    ));
}

#[test]
fn an_arbitrary_trailing_field_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse("./data:/app:z,nocopy"),
        Err(VolumeMappingError::UnsupportedField { .. })
    ));
}

#[test]
fn a_trailing_field_after_a_drive_path_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse(r"C:\data:/data:ro"),
        Err(VolumeMappingError::UnsupportedField { .. })
    ));
}

#[test]
fn the_diagnostic_of_a_trailing_field_names_it() {
    let error = VolumeMapping::parse("./data:/app:ro").expect_err("a third field is rejected");
    let message = error.to_string();
    assert!(
        message.contains("ro"),
        "the diagnostic must name the unsupported field, got `{message}`"
    );
}

#[test]
fn an_empty_mapping_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse(""),
        Err(VolumeMappingError::Empty)
    ));
}

#[test]
fn a_mapping_without_a_separator_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse("pgdata"),
        Err(VolumeMappingError::MissingTarget { .. })
    ));
}

#[test]
fn a_drive_path_without_a_target_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse(r"C:\project\data"),
        Err(VolumeMappingError::MissingTarget { .. })
    ));
}

#[test]
fn an_empty_source_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse(":/data"),
        Err(VolumeMappingError::EmptyComponent { .. })
    ));
}

#[test]
fn an_empty_target_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse("pgdata:"),
        Err(VolumeMappingError::EmptyComponent { .. })
    ));
}

// Braces are unsafe in the export templates, which interpolate service values
// into Helm and Compose documents.
#[test]
fn a_braced_volume_name_is_rejected() {
    assert!(matches!(
        VolumeMapping::parse("{{evil}}:/data"),
        Err(VolumeMappingError::InvalidName { .. })
    ));
}

#[test]
fn a_braced_host_path_is_not_treated_as_a_volume_name() {
    // A host path is not interpolated as a volume name, so the brace rule
    // does not apply to it. It must still parse as a host path.
    assert_eq!(host_path("./{{dir}}:/data"), "./{{dir}}");
}

#[test]
fn every_error_renders_a_message_naming_the_offending_value() {
    let cases = [
        "",
        "pgdata",
        ":/data",
        "pgdata:",
        "./data:/app:ro",
        "{{evil}}:/data",
    ];
    for mapping in cases {
        let error = VolumeMapping::parse(mapping).expect_err("case is invalid");
        assert!(
            !error.to_string().is_empty(),
            "`{mapping}` produced an empty diagnostic"
        );
    }
}
