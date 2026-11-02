//! The real-install tests' weapon catalog. `zhbo_core::weapons::install_catalog` is once per
//! process, and the ignored suite runs its tests in one process on parallel threads: a test that
//! installed another table (the class side alone, without the art locks and converted
//! combinations) used to win the race now and then, and every later test silently played on it.
//! Every real-install test that needs the catalog installs it here: the table the launcher ships
//! (`generals::files`), converted once per process.

use std::path::Path;
use std::sync::OnceLock;

/// Install the shipped weapon table (once per process); panics if another table got in first.
pub(crate) fn install_shipped(bo2_dir: &Path) {
    static TEXT: OnceLock<String> = OnceLock::new();
    static INSTALLED: OnceLock<()> = OnceLock::new();
    let text = TEXT.get_or_init(|| {
        let (files, _) = crate::generals::files(bo2_dir).expect("the launcher's conversion");
        let table = files.iter().find(|(p, _)| p == crate::weapons::TABLE_PATH).expect("the shipped weapon table");
        String::from_utf8_lossy(&table.1).into_owned()
    });
    INSTALLED.get_or_init(|| {
        if let Err(e) = zhbo_core::weapons::install_catalog(text) {
            panic!("{e}: another weapon table was installed first in this process; real-install tests install through test_catalog::install_shipped");
        }
    });
}
