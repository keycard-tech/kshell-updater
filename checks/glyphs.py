import pathlib
import re
import sys
import unicodedata

DRAWN = [(0x00A0, 0x00FF), (0x0100, 0x017F), (0x2000, 0x206F), (0x2190, 0x2193)]

FILES = (
    [pathlib.Path('frontend/index.html')]
    + sorted(pathlib.Path('frontend/css').glob('*.css'))
    + sorted(pathlib.Path('frontend/src').rglob('*.ts'))
)

def shown(source, name):
    if name.endswith('.html'):
        source = re.sub(r'<!--.*?-->', '', source, flags=re.S)
    source = re.sub(r'/\*.*?\*/', '', source, flags=re.S)
    return re.sub(r'^[ \t]*//.*$', '', source, flags=re.M)


missing = {}
for path in FILES:
    for line, text in enumerate(shown(path.read_text(), path.name).splitlines(), 1):
        for ch in text:
            if ord(ch) > 127 and not any(low <= ord(ch) <= high for low, high in DRAWN):
                missing.setdefault(ch, []).append(f'{path}:{line}')

for ch, where in sorted(missing.items()):
    print(f'U+{ord(ch):04X} {ch} {unicodedata.name(ch, "unnamed")} - {" ".join(where[:3])}')
if missing:
    print(f'{len(missing)} character(s) no mac font has: use a plain one, or draw it in css or svg.')
    sys.exit(1)
print(f'{len(FILES)} files, every character drawn by both engines')
