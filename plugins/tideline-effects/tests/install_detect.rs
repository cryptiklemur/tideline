use tideline_effects::install::{
    detect_distro, install_command, Distro, InstallProbe,
};

#[test]
fn arch_maps_to_pacman() {
    let cmd = install_command(Distro::Arch);
    assert!(cmd.starts_with("pacman -S --noconfirm"));
    assert!(cmd.contains("carla"));
    assert!(cmd.contains("lsp-plugins-vst3"));
}

#[test]
fn debian_maps_to_apt_with_umbrella_lsp() {
    let cmd = install_command(Distro::DebianUbuntu);
    assert!(cmd.contains("apt-get"));
    assert!(cmd.contains("carla"));
    assert!(cmd.contains("lsp-plugins"));
    assert!(!cmd.contains("lsp-plugins-vst3"));
}

#[test]
fn fedora_maps_to_dnf() {
    let cmd = install_command(Distro::Fedora);
    assert!(cmd.contains("dnf"));
}

#[test]
fn detect_distro_handles_synthetic_os_release() {
    let arch = "ID=arch\nNAME=\"Arch Linux\"\n";
    assert_eq!(detect_distro(arch), Some(Distro::Arch));
    let ubuntu = "ID=ubuntu\nID_LIKE=debian\n";
    assert_eq!(detect_distro(ubuntu), Some(Distro::DebianUbuntu));
    let fedora = "ID=fedora\n";
    assert_eq!(detect_distro(fedora), Some(Distro::Fedora));
}

#[test]
fn install_probe_treats_missing_carla_as_needs_install() {
    let probe = InstallProbe {
        carla_path: None,
        lsp_plugins_present: false,
    };
    assert!(probe.needs_install());
}

#[test]
fn install_probe_treats_both_present_as_ok() {
    let probe = InstallProbe {
        carla_path: Some("/usr/bin/carla".into()),
        lsp_plugins_present: true,
    };
    assert!(!probe.needs_install());
}
