# Rentemester appikon, første version

`rentemester-app-icon-v1.png` er den godkendte, uændrede rastermaster: et
geometrisk, elfenbensfarvet R på en mørkegrøn flade med afrundede hjørner.
`rentemester-app-icon-v1.icns` er en afledt macOS-repræsentation af denne master.

Genopret ICNS på macOS med `sh scripts/rust/build-app-icon.sh`. Scriptet bruger
`sips` til 16, 32, 128, 256 og 512 px samt hver størrelses Retina-variant og
`iconutil` til ICNS. Det ændrer ikke PNG-masteren eller de ældre filer i
`assets/brand/`; der fremstilles intet nyt design.

`scripts/release/assemble-native-macos.py` vælger denne ICNS som standard og
kopierer den til `Contents/Resources/Rentemester.icns`. `Contents/Info.plist`
henviser til filen med `CFBundleIconFile`. En eksplicit `--icon` kan fortsat
vælge en anden fil. Desktop-byggescriptet bruger nu samlerens standardvalg.
Webmanifestet er ikke ændret.

ICNS-format, størrelser og pakkens fil-/hashreference kan kontrolleres uden at
starte appen. Dette dokumenterer ikke visning i Finder/Dock, signering,
notarisering, installation eller et gennemført native brugerforløb.

Fremstillet med det indbyggede `image_gen`-værktøj, 19. oktober 2026.

## Anvendt prompt

Use case: logo-brand. Create a finished app icon for Rentemester, a Danish
bookkeeping desktop application. One single square icon, straight-on, filling
the image, not a presentation board or mockup. A beautiful restrained Nordic
modernist identity: a bold custom ivory R monogram on deep forest-green
rounded-square tile. The R has precise geometric construction, a generous open
counter, sturdy proportions, softly refined corners and a memorable diagonal
leg; subtly evoke two balanced ledger entries in the construction without
adding separate decorative symbols. Flat vector-like graphic, immaculate clean
edges, excellent legibility at 24 px. Center mark with generous consistent
padding, optically balanced. Calm, trustworthy, sophisticated and distinctive.
Very subtle depth only on the green tile if useful. No extra words, no currency
signs, no charts or arrows, no coins, no illustrations, no watermark. Deliver
only the single finished app icon.
