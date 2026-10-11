use mote_client_core::gallery::GalleryPage;

#[derive(Default)]
pub struct LaunchOptions {
    pub page: Option<GalleryPage>,
    pub reduced_motion: bool,
}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut options = Self::default();
        let mut args = args.into_iter().peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mote-page" => {
                    if let Some(name) = args.next_if(|value| !value.starts_with("--")) {
                        if let Some(page) = GalleryPage::ALL
                            .into_iter()
                            .find(|page| page.name().eq_ignore_ascii_case(&name))
                        {
                            options.page = Some(page);
                        }
                    }
                }
                "--mote-fixture" => {
                    if args.next_if(|value| !value.starts_with("--")).as_deref()
                        == Some("slider-layout")
                    {
                        options.page = Some(GalleryPage::Fixture);
                    }
                }
                "--mote-reduced-motion" => options.reduced_motion = true,
                _ => {}
            }
        }
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_launch_options_choose_page_and_reduced_motion() {
        let options = LaunchOptions::parse(
            ["Mote", "--mote-page", "form", "--mote-reduced-motion"].map(str::to_owned),
        );
        assert_eq!(options.page, Some(GalleryPage::Form));
        assert!(options.reduced_motion);
    }

    #[test]
    fn checked_in_fixture_can_be_selected_without_a_path() {
        let options =
            LaunchOptions::parse(["Mote", "--mote-fixture", "slider-layout"].map(str::to_owned));
        assert_eq!(options.page, Some(GalleryPage::Fixture));
    }

    #[test]
    fn unknown_arguments_and_missing_values_preserve_defaults() {
        for args in [
            vec!["Mote"],
            vec!["Mote", "--mote-page", "unknown"],
            vec!["Mote", "--mote-page"],
            vec!["Mote", "--mote-fixture", "/private/file"],
        ] {
            let options = LaunchOptions::parse(args.into_iter().map(str::to_owned));
            assert_eq!(options.page, None);
            assert!(!options.reduced_motion);
        }
    }

    #[test]
    fn missing_values_do_not_swallow_later_recognized_flags() {
        for flag in ["--mote-page", "--mote-fixture"] {
            let options = LaunchOptions::parse(
                ["Mote", flag, "--mote-reduced-motion", "--mote-page", "form"].map(str::to_owned),
            );
            assert!(options.reduced_motion);
            assert_eq!(options.page, Some(GalleryPage::Form));
        }
    }

    #[test]
    fn invalid_later_values_do_not_erase_a_valid_page_selection() {
        for tail in [
            vec!["--mote-page", "unknown"],
            vec!["--mote-page"],
            vec!["--mote-fixture", "unknown"],
        ] {
            let args = ["Mote", "--mote-page", "form"]
                .into_iter()
                .chain(tail)
                .map(str::to_owned);
            assert_eq!(LaunchOptions::parse(args).page, Some(GalleryPage::Form));
        }
    }
}
