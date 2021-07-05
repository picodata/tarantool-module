import sys
import os

sys.path.insert(0, os.path.abspath(''))

master_doc = 'doc/cartridge_kubernetes_guide/index'

source_suffix = '.rst'

project = u'Tarantool-operator'

exclude_patterns = [
    'doc/locale',
    'doc/output',
    'doc/README.md',
    'doc/cleanup.py',
    'doc/requirements.txt',
]

language = 'en'
locale_dirs = ['./doc/locale']
gettext_compact = False
gettext_location = True
