#pragma once

#include "Generating/ICodeTemplate.h"

// Conquer Ops: prints every struct reachable from the zone's asset types as Rust constants (size and
// field offsets, for the game's 32-bit layout), for reading assets' plain fields from a loaded zone.
class RustLayoutTemplate final : public ICodeTemplate
{
public:
    std::vector<CodeTemplateFile> GetFilesToRenderOncePerTemplate(const OncePerTemplateRenderingContext& context) override;
    void RenderOncePerTemplateFile(std::ostream& stream, CodeTemplateFileTag fileTag, const OncePerTemplateRenderingContext& context) override;

    std::vector<CodeTemplateFile> GetFilesToRenderOncePerAsset(const OncePerAssetRenderingContext& context) override;
    void RenderOncePerAssetFile(std::ostream& stream, CodeTemplateFileTag fileTag, const OncePerAssetRenderingContext& context) override;
};
