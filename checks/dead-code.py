import pathlib
import re
import sys

COMPOSED = {
    'ksh__status--': 'frontend/src/update/rows.ts',
    'ksh__item-state--': 'frontend/src/update/transfer.ts',
}

CSS = sorted((pathlib.Path('frontend/css')).glob('*.css'))
TS = sorted(pathlib.Path('frontend/src').rglob('*.ts'))
STRINGS = pathlib.Path('frontend/src/strings.ts')
HTML = pathlib.Path('frontend/index.html')
HARNESS = pathlib.Path('checks/webkit-check.py')

read = lambda paths: '\n'.join(p.read_text() for p in paths)
text = lambda path: path.read_text()
classes_in = lambda *sources: set(
    name for source in sources for name in re.findall(r'\bksh__[a-z0-9_-]+', source)
)
composed = lambda name: any(name.startswith(prefix) for prefix in COMPOSED)


def main():
    css, ts, html, harness = read(CSS), read(TS), text(HTML), text(HARNESS)
    everything = css + ts + html + harness

    declared = {}
    for path in TS:
        for name in re.findall(
            r'export (?:const|function|class|type|interface|enum) (\w+)', path.read_text()
        ):
            declared[name] = path
    elsewhere = lambda home: '\n'.join(p.read_text() for p in TS if p != home) + html + harness

    leafs = set(re.findall(r'^\s+(\w+):', text(STRINGS), re.M))
    found = {
        'exports nothing imports': {
            name for name, home in declared.items() if not re.search(rf'\b{name}\b', elsewhere(home))
        },
        'strings nothing reads': {key for key in leafs if not re.search(rf'\b{key}\b', everything)},
        'ids nothing looks at': {
            i for i in re.findall(r'id="([\w-]+)"', html) if i not in everything
        },
        'classes no element carries': {
            name for name in classes_in(css) - classes_in(html, ts, harness) if not composed(name)
        },
        'classes carried with no rule': {
            name for name in classes_in(html, ts) - classes_in(css) if not composed(name)
        },
    }

    reported = False
    for title, names in found.items():
        if names:
            reported = True
            print(f'\n{title}:')
            print('\n'.join(f'  {name}' for name in sorted(names)))
    print('\nnothing left behind' if not reported else '')
    return 1 if reported else 0


if __name__ == '__main__':
    sys.exit(main())
