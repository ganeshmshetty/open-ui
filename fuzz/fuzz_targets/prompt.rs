#![no_main]
use genui_prompt::{PromptOptions, generate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(options) = serde_json::from_slice::<PromptOptions>(data) else {
        return;
    };
    let Ok(library) = genui_std::std_library() else {
        return;
    };
    let prompt = generate(library.spec(), &options);
    let again = generate(library.spec(), &options);
    assert_eq!(prompt.text, again.text);
    assert_eq!(prompt.digest, again.digest);
    assert!(!prompt.text.contains('\r'));
    assert!(prompt.text.ends_with('\n'));
});
