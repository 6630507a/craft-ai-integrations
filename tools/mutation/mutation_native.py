"""Deliberate-bug cases for group `nf`: planning and offering a built-in runtime's own files.

Loaded by mutation_check.py. Each case breaks one rule and names the test that should notice.
Format: (name, file, old, new, package, test target, test filter).
"""
F = 'crates/ai-features/src/'
FE = 'craft-ai-features'

NF = [
    ('native-plan-ignores-usable', F + 'plan.rs', 'let recipe = feature.first_usable_recipe();', 'let recipe = feature.locals.first();', FE, 'native_setup', 'cannot_run_yet'),
    ('first-usable-recipe-takes-the-first', F + 'feature.rs', 'self.locals.iter().find(|r| r.usable)', 'self.locals.first()', FE, 'native_setup', 'first_usable_recipe'),
    ('native-offer-takes-the-first-recipe', F + 'setup.rs', 'feature.first_usable_recipe()?', 'feature.locals.first()?', FE, 'setup', 'unusable_first_recipe'),
    ('native-option-from-every-recipe', F + 'options.rs', 'if let Some(recipe) = feature.first_usable_recipe() {', 'for recipe in &feature.locals {', FE, 'native_setup', 'first_usable_recipe'),
    ('native-plan-takes-a-route-without-files', F + 'plan.rs', '.find(|n| n.provider == provider && !n.models.is_empty())\n    }) else {\n        return out;\n    };\n    fill_files(&mut out, &route.models, inventory);\n    check_disk', '.find(|n| n.provider == provider)\n    }) else {\n        return out;\n    };\n    fill_files(&mut out, &route.models, inventory);\n    check_disk', FE, 'native_setup', 'unfinished_or_unknown'),
    ('native-plan-takes-any-provider', F + 'plan.rs', '.find(|n| n.provider == provider && !n.models.is_empty())\n    }) else {\n        return out;\n    };\n    fill_files(&mut out, &route.models, inventory);\n    check_disk', '.find(|n| !n.models.is_empty())\n    }) else {\n        return out;\n    };\n    fill_files(&mut out, &route.models, inventory);\n    check_disk', FE, 'native_setup', 'unfinished_or_unknown'),
    ('native-plan-skips-the-disk-check', F + 'plan.rs', '    fill_files(&mut out, &route.models, inventory);\n    check_disk(&mut out, inventory);', '    fill_files(&mut out, &route.models, inventory);', FE, 'native_setup', 'free_space'),
    ('native-offer-ignores-the-provider', F + 'setup.rs', 'selected_provider.is_none_or(|provider| n.provider == provider)', 'true', FE, 'setup', 'selected_provider'),
    ('built-in-offer-compares-comfyui-files', F + 'setup.rs', 'let compared = if let Some(native) = native {', 'let compared = if let Some(native) = native.filter(|_| false) {', FE, 'setup', 'needs_only_the_model'),
    ('option-offered-for-a-route-without-files', F + 'options.rs', 'for native in recipe.native.iter().filter(|n| !n.models.is_empty()) {', 'for native in recipe.native.iter() {', FE, 'native_setup', 'not_unbuilt_burn'),
    ('native-options-before-later-locals', F + 'options.rs', 'locals.extend(natives);', 'locals.splice(0..0, natives);', FE, 'native_setup', 'follow_every_local'),
    ('built-in-offer-uses-the-comfyui-terms', F + 'setup.rs', 'agreement: agreement_for(route),', 'agreement: agreement(),', FE, 'setup', 'agrees_to_its_own'),
    ('native-option-loses-its-model-id', F + 'options.rs', 'model: native.model.clone(),', 'model: String::new(),', FE, 'native_setup', 'candle_with_its_own_size'),
]
