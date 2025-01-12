import os
import re

with open("manifest-template.yml") as f:
    manifest_template = f.read()
with open("template.desktop") as f:
    desktop_template = f.read()

variables = {
    "version": os.getenv("VERSION", ""),
    "flatpak_id": os.environ["FLATPAK_ID"],
    "app_name": os.environ["LAUNCHER_NAME"],
    "app_name_lower": os.environ["LAUNCHER_NAME"]
    .lower()
    .replace("'", "")
    .replace(" ", "_"),
    "app_description": os.getenv("LAUNCHER_DESCRIPTION", ""),
    "flatpak_keywords": os.getenv("FLATPAK_KEYWORDS", ""),
    "version_manifest_url": os.environ["VERSION_MANIFEST_URL"],
}

for k, v in variables.items():
    rgx = re.compile(r"{{\s*" + k + r"\s*}}")
    manifest_template = rgx.sub(v, manifest_template)
    desktop_template = rgx.sub(v, desktop_template)

with open(variables["flatpak_id"] + ".yml", "w") as f:
    f.write(manifest_template)
with open(variables["flatpak_id"] + ".desktop", "w") as f:
    f.write(desktop_template)
