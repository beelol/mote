use mote_client_core::gallery::GalleryPage;

#[derive(Default)]
pub struct LaunchOptions {
    pub page: Option<GalleryPage>,
    pub reduced_motion: bool,
}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mote-page" => {
                    if let Some(name) = args.next() {
                        options.page = GalleryPage::ALL
                            .into_iter()
                            .find(|page| page.name().eq_ignore_ascii_case(&name));
                    }
                }
                "--mote-fixture" => {
                    if args.next().as_deref() == Some("slider-layout") {
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
}
