fn main() {
    // Path is relative to this crate's manifest dir
    // (playerbot-rs/crates/cmangos-sys/), three levels up to repo root,
    // then into cpp_wrapper/.
    let ffi_header = "../../../cpp_wrapper/botffi.h";

    println!("cargo:rerun-if-changed={ffi_header}");

    let bindings = bindgen::Builder::default()
        .header(ffi_header)
        // Enumerate every POD type explicitly so transitive closure
        // through BotCallbacks is not load-bearing.
        .allowlist_type("BotCallbacks")
        .allowlist_type("BotHandle")
        .allowlist_type("UnitHandle")
        .allowlist_type("BotPosition")
        .allowlist_type("BotSafePosition")
        .allowlist_type("BotWorldSnapshot")
        .allowlist_type("BotUnitSnapshot")
        .allowlist_type("BotAuraInfo")
        .allowlist_type("BotThreatEntry")
        .allowlist_type("BotDispelTarget")
        .allowlist_type("BotInventoryItem")
        .allowlist_type("BotMailSummary")
        .allowlist_type("BotQuestInfo")
        .allowlist_type("BotReputationEntry")
        .allowlist_type("BotSkillEntry")
        .allowlist_type("BotSpellInfo")
        .allowlist_type("BotTalentEntry")
        .allowlist_type("BotTaxiNode")
        .allowlist_type("BotTravelDest")
        .allowlist_type("LoginCallbacks")
        .allowlist_type("BotCandidateInfo")
        .allowlist_type("BotRealPlayerInfo")
        .allowlist_type("ItemCallbacks")
        .allowlist_type("BotItemPrototype")
        .allowlist_type("BotItemProtoStat")
        .allowlist_type("BotItemProtoDamage")
        .allowlist_type("BotItemProtoSocket")
        .allowlist_type("BotItemProtoSpellRef")
        .allowlist_type("BotSpellEntryInfo")
        .allowlist_type("BotWeightScaleRow")
        .allowlist_type("BotWeightScaleStatRow")
        .allowlist_type("BotQuestItemRow")
        .allowlist_type("BotVendorItemRow")
        .allowlist_type("BotItemEnchantmentRow")
        .allowlist_type("BotEnchantInfo")
        .allowlist_type("BotItemRarityRow")
        .allowlist_type("BotGemPropertiesRow")
        .allowlist_type("BotPlayerItemCtx")
        .allowlist_var("PLAYERBOT_HOLDER_.*")
        .allowlist_var("PLAYERBOT_QUEST_.*")
        // The Rust → C exports are implemented in the playerbot crate;
        // bindgen must not generate them here.
        .blocklist_function("playerbot_.*")
        // Use ::core::* paths for #![no_std] compatibility.
        .use_core()
        .derive_debug(true)
        .derive_copy(true)
        .derive_default(true)
        .generate()
        .expect("bindgen failed to generate FFI bindings from botffi.h");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}
