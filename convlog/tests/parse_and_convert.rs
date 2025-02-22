mod testdata;

use convlog::*;
use testdata::{TESTDATA, TestCase};

#[test]
fn test_parse_and_convert() {
    TESTDATA.iter().for_each(|TestCase { desc, data }| {
        let tenhou_log = tenhou::Log::from_json_str(data)
            .unwrap_or_else(|_| panic!("failed to parse tenhou log (case: {desc})"));
        let mjai_log = tenhou_to_mjai(&tenhou_log)
            .unwrap_or_else(|_| panic!("failed to transform tenhou log (case: {desc})"));

        assert!(mjai_log.len() >= 4);
    });
}
