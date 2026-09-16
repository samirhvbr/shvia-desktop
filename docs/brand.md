# Portal identity

The approved Portal artwork is maintained in the SHVIA-WEB repository under `brand/atual`. Its `brand/versao1` directory preserves the former graphics with hashes and source commits. The palette is cyan `#34B3EC` and navy `#0B0F17`; no gradients or glow.

Run SHVIA-WEB `brand/tools/export.cjs <workspace-root>` with Node and the `sharp` package available through NODE_PATH, then `brand/tools/export-native.py <workspace-root>` with Python and Pillow. The export tool updates sibling desktop, mobile, site and Workspace checkouts, so check their status before running it. Do not run the exporter against unrelated work in progress.

Desktop uses a rounded cyan tile, a transparent monochrome tray template, and the Portal splash symbol. Mobile uses an opaque square iOS source, transparent Android adaptive foreground in the safe center zone, and cyan backgrounds. Browser favicons use a separate drawing with an enlarged core on the 16-pixel grid.

Updating source assets does not replace installed applications or submit store builds. Rebuild and distribute through the existing release process. Site social images are rendered by `bin/make-images.sh` in the site repository after editing its OG templates.
