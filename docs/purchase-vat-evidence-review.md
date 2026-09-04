# Review af købsmoms ved formel fakturamangel

En standardfaktura skal normalt have købers navn og adresse. Rentemester ændrer
aldrig den udstedte faktura eller kalder den en forenklet faktura.

Når en dansk standardfaktura sandfærdigt er markeret som ufuldstændig, kan den
kun bruges til købsmoms efter en særskilt, append-only vurdering. Vurderingen
kræver gyldig leverandøridentitet, påført dansk 25 % moms, en matching udgående
DKK-bankpost, et SHA-256-identificeret erhvervsbevis, hash-bundet
virksomhedskontekst, autentificeret principal, actor og eksplicit bekræftelse.
Samme input returnerer samme review; nyt bevis kræver eksplicit supersession.

Grundlaget er momsloven § 37 og § 36 a, momsbekendtgørelsens fakturakrav samt
Skattestyrelsens D.A.11.1.7. EU-Domstolens C-516/14 *Barlis* fastslår, at en
formel fakturamangel ikke alene kan afskære fradrag, hvis myndigheden har alle
oplysninger til at efterprøve de materielle betingelser. Det er ikke et
ubetinget fradrag eller en erstatning for bevis for reel levering og
erhvervsmæssig anvendelse.
