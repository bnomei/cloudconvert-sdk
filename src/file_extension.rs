use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! file_extensions {
    ($($variant:ident => $extension:literal),+ $(,)?) => {
        /// Supported CloudConvert file extension tokens.
        ///
        /// Values serialize to the lowercase extension token CloudConvert expects,
        /// without a leading dot. The list is derived from CloudConvert's
        /// operations metadata endpoint.
        ///
        /// ```
        /// use cloudconvert_sdk::FileExtension;
        ///
        /// assert_eq!(FileExtension::Pdf.as_str(), "pdf");
        /// assert_eq!(".TAR.GZ".parse::<FileExtension>().unwrap(), FileExtension::TarGz);
        /// assert_eq!(
        ///     serde_json::to_value(FileExtension::SevenZ).unwrap(),
        ///     serde_json::json!("7z")
        /// );
        /// ```
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[non_exhaustive]
        pub enum FileExtension {
            $($variant,)+
        }

        impl FileExtension {
            /// All known file extensions in sorted order.
            pub const ALL: &'static [Self] = &[
                $(Self::$variant,)+
            ];

            /// Returns the extension token CloudConvert expects.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $extension,)+
                }
            }

            /// Parses a CloudConvert extension token.
            ///
            /// Leading dots, surrounding whitespace and ASCII case differences are
            /// normalized before matching.
            ///
            /// ```
            /// use cloudconvert_sdk::FileExtension;
            ///
            /// assert_eq!(FileExtension::parse(" .PDF ").unwrap(), FileExtension::Pdf);
            /// ```
            pub fn parse(value: &str) -> Result<Self, ParseFileExtensionError> {
                value.parse()
            }
        }

        impl FromStr for FileExtension {
            type Err = ParseFileExtensionError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let normalized = normalize_file_extension(value);

                match normalized.as_str() {
                    $($extension => Ok(Self::$variant),)+
                    _ => Err(ParseFileExtensionError::new(value)),
                }
            }
        }
    };
}

file_extensions! {
    ThreeFr => "3fr",
    ThreeG2 => "3g2",
    ThreeGp => "3gp",
    ThreeGpp => "3gpp",
    SevenZ => "7z",
    Aac => "aac",
    Abw => "abw",
    Ac3 => "ac3",
    Ace => "ace",
    Ai => "ai",
    Aif => "aif",
    Aifc => "aifc",
    Aiff => "aiff",
    Alz => "alz",
    Amr => "amr",
    Arc => "arc",
    Arj => "arj",
    Arw => "arw",
    Au => "au",
    Avi => "avi",
    Avif => "avif",
    Azw => "azw",
    Azw3 => "azw3",
    Azw4 => "azw4",
    Bmp => "bmp",
    Bz => "bz",
    Bz2 => "bz2",
    Cab => "cab",
    Caf => "caf",
    Cavs => "cavs",
    Cbc => "cbc",
    Cbr => "cbr",
    Cbz => "cbz",
    Cdr => "cdr",
    Cgm => "cgm",
    Chm => "chm",
    Cpio => "cpio",
    Cr2 => "cr2",
    Cr3 => "cr3",
    Crw => "crw",
    Csv => "csv",
    Dcr => "dcr",
    Deb => "deb",
    Djvu => "djvu",
    Dmg => "dmg",
    Dng => "dng",
    Doc => "doc",
    Docm => "docm",
    Docx => "docx",
    Dot => "dot",
    Dotx => "dotx",
    Dps => "dps",
    Dss => "dss",
    Dv => "dv",
    Dvr => "dvr",
    Dwf => "dwf",
    Dwg => "dwg",
    Dxf => "dxf",
    Emf => "emf",
    Eml => "eml",
    Eot => "eot",
    Eps => "eps",
    Epub => "epub",
    Erf => "erf",
    Et => "et",
    Fb2 => "fb2",
    Flac => "flac",
    Flv => "flv",
    Gif => "gif",
    Gz => "gz",
    Heic => "heic",
    Heif => "heif",
    Htm => "htm",
    Html => "html",
    Htmlz => "htmlz",
    Hwp => "hwp",
    Hwpx => "hwpx",
    Icns => "icns",
    Ico => "ico",
    Img => "img",
    Iso => "iso",
    Jar => "jar",
    Jfif => "jfif",
    Jpeg => "jpeg",
    Jpg => "jpg",
    Key => "key",
    Lha => "lha",
    Lit => "lit",
    Lrf => "lrf",
    Lwp => "lwp",
    Lz => "lz",
    Lzma => "lzma",
    Lzo => "lzo",
    M2ts => "m2ts",
    M4a => "m4a",
    M4b => "m4b",
    M4v => "m4v",
    Md => "md",
    Mkv => "mkv",
    Mobi => "mobi",
    Mod => "mod",
    Mos => "mos",
    Mov => "mov",
    Mp3 => "mp3",
    Mp4 => "mp4",
    Mpeg => "mpeg",
    Mpg => "mpg",
    Mrw => "mrw",
    Mts => "mts",
    Mxf => "mxf",
    Nef => "nef",
    Numbers => "numbers",
    Odd => "odd",
    Odg => "odg",
    Odp => "odp",
    Ods => "ods",
    Odt => "odt",
    Oeb => "oeb",
    Oga => "oga",
    Ogg => "ogg",
    Ogv => "ogv",
    Opus => "opus",
    Orf => "orf",
    Otf => "otf",
    Pages => "pages",
    Pdb => "pdb",
    Pdf => "pdf",
    Pef => "pef",
    Pml => "pml",
    Png => "png",
    Pot => "pot",
    Potx => "potx",
    Ppm => "ppm",
    Pps => "pps",
    Ppsx => "ppsx",
    Ppt => "ppt",
    Pptm => "pptm",
    Pptx => "pptx",
    Prc => "prc",
    Ps => "ps",
    Psb => "psb",
    Psd => "psd",
    Pub => "pub",
    Raf => "raf",
    Rar => "rar",
    Raw => "raw",
    Rb => "rb",
    Rm => "rm",
    Rmvb => "rmvb",
    Rpm => "rpm",
    Rst => "rst",
    Rtf => "rtf",
    Rw2 => "rw2",
    Rz => "rz",
    Sda => "sda",
    Sdc => "sdc",
    Sdw => "sdw",
    Sf2 => "sf2",
    Sfark => "sfark",
    Sk => "sk",
    Sk1 => "sk1",
    Snb => "snb",
    Svg => "svg",
    Svgz => "svgz",
    Swf => "swf",
    Tar => "tar",
    Tar7z => "tar.7z",
    TarBz => "tar.bz",
    TarBz2 => "tar.bz2",
    TarGz => "tar.gz",
    TarLzo => "tar.lzo",
    TarXz => "tar.xz",
    TarZ => "tar.z",
    Tbz => "tbz",
    Tbz2 => "tbz2",
    Tcr => "tcr",
    Tex => "tex",
    Tga => "tga",
    Tgz => "tgz",
    Tif => "tif",
    Tiff => "tiff",
    Ts => "ts",
    Ttf => "ttf",
    Txt => "txt",
    Txtz => "txtz",
    Tz => "tz",
    Tzo => "tzo",
    Vob => "vob",
    Voc => "voc",
    Vsd => "vsd",
    Vtt => "vtt",
    Wav => "wav",
    Weba => "weba",
    Webm => "webm",
    Webp => "webp",
    Wma => "wma",
    Wmf => "wmf",
    Wmv => "wmv",
    Woff => "woff",
    Woff2 => "woff2",
    Wpd => "wpd",
    Wps => "wps",
    Wtv => "wtv",
    X3f => "x3f",
    Xcf => "xcf",
    Xls => "xls",
    Xlsm => "xlsm",
    Xlsx => "xlsx",
    Xps => "xps",
    Xz => "xz",
    Z => "z",
    Zabw => "zabw",
    Zip => "zip",
}

impl AsRef<str> for FileExtension {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for FileExtension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<FileExtension> for String {
    fn from(value: FileExtension) -> Self {
        value.as_str().to_string()
    }
}

impl From<&FileExtension> for String {
    fn from(value: &FileExtension) -> Self {
        value.as_str().to_string()
    }
}

impl Serialize for FileExtension {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FileExtension {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a string into [`FileExtension`] fails.
///
/// ```
/// use cloudconvert_sdk::FileExtension;
///
/// let error = FileExtension::parse("not-real").unwrap_err();
/// assert_eq!(error.extension(), "not-real");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseFileExtensionError {
    extension: String,
}

impl ParseFileExtensionError {
    fn new(extension: impl Into<String>) -> Self {
        Self {
            extension: extension.into(),
        }
    }

    /// The unrecognized extension as provided to the parser.
    pub fn extension(&self) -> &str {
        &self.extension
    }
}

impl fmt::Display for ParseFileExtensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unsupported CloudConvert file extension: {}",
            self.extension
        )
    }
}

impl std::error::Error for ParseFileExtensionError {}

pub(crate) fn normalize_file_extension(value: impl Into<String>) -> String {
    value
        .into()
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}
