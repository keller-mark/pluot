from zarr.storage import LocalStore

from pluot_widget import PluotWasmWidget


def test_stores_accept_instances_and_metadata(tmp_path):
    metadata = {"store_type": "LocalStore", "store_params": {"path": str(tmp_path)}}
    widget = PluotWasmWidget(stores={"from_instance": LocalStore(tmp_path), "from_metadata": metadata})

    assert isinstance(widget._stores["from_metadata"], LocalStore)
    assert widget.stores_metadata["from_metadata"] == metadata
    assert widget.stores_metadata["from_instance"]["store_type"] == "LocalStore"


def test_single_store_accepts_metadata(tmp_path):
    metadata = {"store_type": "LocalStore", "store_params": {"path": str(tmp_path)}}
    widget = PluotWasmWidget(store=metadata, store_name="my_store")

    assert widget.store_name == "my_store"
    assert isinstance(widget._stores["my_store"], LocalStore)
    assert widget.stores_metadata == {"my_store": metadata}


def test_add_store_accepts_metadata(tmp_path):
    widget = PluotWasmWidget()
    widget.add_store("added", {"store_type": "LocalStore", "store_params": {"path": str(tmp_path)}})

    assert isinstance(widget._stores["added"], LocalStore)


def test_render_to_widget_is_exported_from_pluot(tmp_path):
    from pluot import render_to_widget

    metadata = {"store_type": "LocalStore", "store_params": {"path": str(tmp_path)}}
    widget = render_to_widget(stores={"my_store": metadata}, width=320)

    assert isinstance(widget, PluotWasmWidget)
    assert widget.width == 320
    assert widget.stores_metadata == {"my_store": metadata}
