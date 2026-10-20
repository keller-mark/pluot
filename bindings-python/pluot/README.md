This is a meta-Python package which uses extras_require / optional dependencies metadata to allow a user to install either the Rust bindings, the Anywidget, or both:

```sh
pip install pluot # neither, only re-exports things from pluot_core
pip install pluot[widget]
pip install pluot[bindings]
pip install pluot[widget,bindings] # both
```
