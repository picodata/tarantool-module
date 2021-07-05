[![Crowdin](https://badges.crowdin.net/tarantool-operator/localized.svg)](https://crowdin.com/project/tarantool-operator)

# Tarantool Kubernetes Operator documentation
Part of Tarantool documentation, published to 
https://www.tarantool.io/en/doc/latest/book/cartridge/cartridge_kubernetes_guide/

## Create pot files from rst
```bash
python -m sphinx doc doc/locale/en -c doc -b gettext
```

## Create/update po from pot files
```bash
sphinx-intl update -p doc/locale/en -d doc/locale -l ru
```

## Build documentation to doc/output
```bash
python -m sphinx doc doc/output -c doc
```
