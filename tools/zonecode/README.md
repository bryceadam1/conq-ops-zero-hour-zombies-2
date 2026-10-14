# Regenerating the BO2 zone loaders

`crates/zhbo-formats/src/t6zone/generated/` is produced by OpenAssetTools' ZoneCodeGenerator
(GPL-3.0) from its T6 struct definitions and zone-code commands, through `RustLoadTemplate.cpp`
here: a port of OAT's `ZoneLoadTemplate` that prints Rust instead of C++. The runtime the generated
code calls is `crates/zhbo-formats/src/t6zone/mod.rs`.

Last generated from OpenAssetTools commit `068d4cd9f513ff0509b757498712b2ec3dee9463`.

```
git clone https://github.com/Laupetin/OpenAssetTools.git oat && git -C oat checkout <commit>
mkdir build && cd build && ln -s ../oat/src s && cp <repo>/tools/zonecode/Makefile .
cp <repo>/tools/zonecode/RustLoadTemplate.* s/ZoneCodeGeneratorLib/Generating/Templates/
# register the preset in s/ZoneCodeGeneratorLib/Generating/CodeGenerator.cpp:
#   #include "Templates/RustLoadTemplate.h"
#   m_template_mapping["zoneloadrust"] = std::make_unique<RustLoadTemplate>();
mkdir -p gen && printf '#pragma once\n#define GIT_VERSION "local"\n' > gen/GitVersion.h
make -j8
./zcg --no-color -h s/ZoneCode/Game/T6/T6_ZoneCode.h -c s/ZoneCode/Game/T6/T6_Commands.txt -g zoneloadrust -o out
cp out/*.rs <repo>/crates/zhbo-formats/src/t6zone/generated/
```

The Makefile builds the generator with `ARCH_x86` defined so it emits the game's own 32-bit loading
semantics (plain loads into block memory) instead of OAT's 64-bit host conversions. It needs a
C++23 compiler (Apple clang 17 works).

Check a regeneration with `zhbo-assets walk`: every zone in `zone/all` must load with 0 bytes left over.
