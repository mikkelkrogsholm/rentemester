"""Check generated synthetic XML using local official schemas and SaxonC-HE.

Usage: python scripts/verify-oioubl-schemas.py XML_DIR OIOUBL_UNZIP_DIR UBL_2_XSD_DIR
Requires lxml and saxonche, installed outside the application environment.
"""
import sys
from pathlib import Path

from lxml import etree
from saxonche import PySaxonProcessor

if len(sys.argv) != 4:
    raise SystemExit(__doc__)
xml_dir, official, ubl2 = map(Path, sys.argv[1:])
schemas = {
    "2.02": etree.XMLSchema(etree.parse(str(ubl2 / "maindoc/UBL-Invoice-2.0.xsd"))),
    "2.1": etree.XMLSchema(etree.parse(str(official / "Schemas/UBL_v2.1/maindoc/UBL-Invoice-2.1.xsd"))),
}
with PySaxonProcessor(license=False) as processor:
    # SaxonC's optimizer may reject this compiled official stylesheet.
    processor.set_configuration_property("http://saxon.sf.net/feature/optimizationLevel", "0")
    transform = processor.new_xslt30_processor().compile_stylesheet(
        stylesheet_file=str(official / "Stylesheets/OIOUBL_Invoice_Schematron.xsl")
    )
    for version in ("2.02", "2.1"):
        for tax in ("exempt", "vat"):
            for payment in ("bank", "iban"):
                path = xml_dir / f"{version}-{tax}-{payment}.xml"
                schemas[version].assertValid(etree.parse(str(path)))
                report = etree.fromstring(transform.transform_to_string(source_file=str(path)).encode())
                errors = report.xpath('//*[local-name()="Error"]')
                if errors:
                    raise SystemExit(f"{path.name}: {len(errors)} Schematron errors: {etree.tostring(errors[0], encoding='unicode')}")
                print(f"{path.name}: XSD valid, 0 Schematron errors")
