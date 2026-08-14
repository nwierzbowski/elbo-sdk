import struct

import numpy as np
import pytest

from elbo_sdk_rust import TboImportContext, TboExportContext

MAGIC = b"TBO\0"
TBO_VERSION = 4


def align16(n):
    return (n + 15) & ~15


def build_tbo(path, format_index, channel_names, entities):
    # entities: list of (rows, channels) float32 arrays
    if entities:
        data = np.concatenate([e.reshape(-1) for e in entities]).astype("<f4")
    else:
        data = np.zeros(0, dtype="<f4")
    names = b"".join(n.encode("utf-8") + b"\0" for n in channel_names)
    data_start = align16(24 + len(names))
    offsets = [data_start]
    for e in entities:
        offsets.append(offsets[-1] + e.nbytes)
    off_bytes = b"".join(struct.pack("<Q", o) for o in reversed(offsets))
    header = struct.pack(
        "<4sIIIII", MAGIC, format_index, TBO_VERSION, 0, len(entities), len(channel_names)
    )
    pad = b"\0" * (data_start - (24 + len(names)))
    path.write_bytes(header + names + pad + data.tobytes() + off_bytes)


def make_dataset(dirpath):
    # scene: 1 entity, 3 rows (one per asset), channels sx/sy
    scene = np.array([[100.0 + i, 200.0 + i] for i in range(3)], dtype="float32")
    build_tbo(dirpath / "scene_0.tbo", 0, ["sx", "sy"], [scene])

    # assets: 3 entities x 2 rows (one per fragment), channels ax/ay
    assets = [
        np.array([[10.0 + i, 50.0 + i], [20.0 + i, 60.0 + i]], dtype="float32")
        for i in range(3)
    ]
    build_tbo(dirpath / "asset_0.tbo", 1, ["ax", "ay"], assets)

    # fragments: 6 entities x 4 rows (points), channels fx/fy
    fragments = [
        np.array(
            [[1.0 + i, 101.0 + i], [2.0 + i, 102.0 + i], [3.0 + i, 103.0 + i], [4.0 + i, 104.0 + i]],
            dtype="float32",
        )
        for i in range(6)
    ]
    build_tbo(dirpath / "fragment_0.tbo", 2, ["fx", "fy"], fragments)


def load_all(ctx, dirpath):
    for name in ("scene_0.tbo", "asset_0.tbo", "fragment_0.tbo"):
        ctx.load_file(str(dirpath / name))


class TestImportHierarchy:
    def test_counts(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        assert h.scene_count == 1
        assert h.asset_count == 3
        assert h.fragment_count == 6

    def test_navigation_and_rows(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()

        assert len(h.Scenes) == 1
        scene = h.Scenes[0]
        assert scene.row_count == 3
        assert scene.channel_names == ["sx", "sy"]

        assets = scene.get_child("Assets")
        assert assets is not None
        assert len(assets) == 3
        for i, asset in enumerate(assets):
            assert asset.row_count == 2
            assert asset.sx == pytest.approx(100.0 + i)
            assert asset.sy == pytest.approx(200.0 + i)

            frags = asset.get_child("Fragments")
            assert frags is not None
            assert len(frags) == 2
            # Asset entity i owns fragment entities 2i and 2i+1 (hierarchy rows
            # partition the fragment collection in order).
            for j, frag in enumerate(frags):
                k = 2 * i + j
                assert frag.row_count == 4
                assert frag.ax == pytest.approx((10.0 if j == 0 else 20.0) + i)
                fx = frag.channel("fx")
                assert fx.dtype == np.float32
                assert fx.tolist() == [1.0 + k, 2.0 + k, 3.0 + k, 4.0 + k]

    def test_channel_and_row(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()

        scene = h.Scenes[0]
        assert scene.channel("sx").tolist() == [100.0, 101.0, 102.0]
        assert scene.channel("sy").tolist() == [200.0, 201.0, 202.0]

        assets = scene.get_child("Assets")
        assert assets is not None
        asset = assets[1]
        assert asset.channel("ax").tolist() == [11.0, 21.0]

        # Asset entity 1 owns fragment entities 2 and 3; [0] is entity 2.
        frags = asset.get_child("Fragments")
        assert frags is not None
        frag = frags[0]
        assert frag.channel("fy").tolist() == [103.0, 104.0, 105.0, 106.0]
        row = np.frombuffer(frag.row(1), dtype="float32")
        assert row.tolist() == [4.0, 104.0]
        with pytest.raises(IndexError):
            frag.row(4)

        with pytest.raises(ValueError):
            frag.channel("nope")

    def test_top_level_has_no_parent_attributes(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        with pytest.raises(AttributeError):
            _ = scene.sx
        with pytest.raises(AttributeError):
            _ = scene.does_not_exist

    def test_selected_entity_idx(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        frags = ctx.get_hierarchy().Fragments
        assert frags.selected_entity_idx == 0
        assert frags.channel("fx")[0] == pytest.approx(1.0)
        frags.selected_entity_idx = 5
        assert frags.channel("fx")[0] == pytest.approx(6.0)
        with pytest.raises(IndexError):
            frags.selected_entity_idx = 6
        with pytest.raises(IndexError):
            frags[6]

    def test_getattr_reflects_selection(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        assets = scene.get_child("Assets")
        assert assets is not None
        for i in (2, 0, 1):
            assets.selected_entity_idx = i
            assert assets.sx == pytest.approx(100.0 + i)

    def test_cross_format_mismatch_rejected(self, tmp_path):
        scene = np.zeros((5, 2), dtype="float32")
        build_tbo(tmp_path / "scene_0.tbo", 0, ["sx", "sy"], [scene])
        assets = [np.zeros((2, 2), dtype="float32") for _ in range(3)]
        build_tbo(tmp_path / "asset_0.tbo", 1, ["ax", "ay"], assets)
        ctx = TboImportContext()
        ctx.load_file(str(tmp_path / "scene_0.tbo"))
        ctx.load_file(str(tmp_path / "asset_0.tbo"))
        with pytest.raises(ValueError):
            ctx.get_hierarchy()

    def test_partial_formats_ok(self, tmp_path):
        fragments = [
            np.zeros((3, 2), dtype="float32") for _ in range(4)
        ]
        build_tbo(tmp_path / "fragment_0.tbo", 2, ["fx", "fy"], fragments)
        ctx = TboImportContext()
        ctx.load_file(str(tmp_path / "fragment_0.tbo"))
        h = ctx.get_hierarchy()
        assert h.fragment_count == 4
        assert h.scene_count == 0
        assert h.asset_count == 0

    def test_channel_layout_mismatch_rejected(self, tmp_path):
        build_tbo(tmp_path / "fragment_0.tbo", 2, ["fx", "fy"], [np.zeros((2, 2), dtype="float32")])
        build_tbo(tmp_path / "fragment_1.tbo", 2, ["fx"], [np.zeros((2, 1), dtype="float32")])
        ctx = TboImportContext()
        ctx.load_file(str(tmp_path / "fragment_0.tbo"))
        ctx.load_file(str(tmp_path / "fragment_1.tbo"))
        with pytest.raises(ValueError):
            ctx.get_hierarchy()

    def test_rows_not_multiple_of_channels_rejected(self, tmp_path):
        path = tmp_path / "fragment_0.tbo"
        header = struct.pack("<4sIIIII", MAGIC, 2, TBO_VERSION, 0, 1, 2)
        names = b"fx\0fy\0"
        data_start = align16(24 + len(names))
        data = np.zeros(5, dtype="<f4").tobytes()
        offsets = struct.pack("<Q", data_start) + struct.pack("<Q", data_start + 8)
        path.write_bytes(header + names + b"\0" * (data_start - 24 - len(names)) + data + offsets)
        ctx = TboImportContext()
        ctx.load_file(str(path))
        with pytest.raises(ValueError):
            ctx.get_hierarchy()

    def test_bad_version_rejected(self, tmp_path):
        path = tmp_path / "fragment_0.tbo"
        make_fragment(path)
        blob = bytearray(path.read_bytes())
        struct.pack_into("<I", blob, 8, 3)
        path.write_bytes(bytes(blob))
        ctx = TboImportContext()
        with pytest.raises(RuntimeError):
            ctx.load_file(str(path))

    def test_bad_magic_rejected(self, tmp_path):
        path = tmp_path / "bad.tbo"
        make_fragment(path)
        blob = bytearray(path.read_bytes())
        blob[0:4] = b"XXXX"
        path.write_bytes(bytes(blob))
        ctx = TboImportContext()
        with pytest.raises(RuntimeError):
            ctx.load_file(str(path))

    def test_truncated_rejected(self, tmp_path):
        path = tmp_path / "trunc.tbo"
        make_fragment(path)
        path.write_bytes(path.read_bytes()[:20])
        ctx = TboImportContext()
        with pytest.raises(RuntimeError):
            ctx.load_file(str(path))

    def test_natural_file_ordering(self, tmp_path):
        # 12 files x 2 entities; lexicographic order would put fragment_10 before fragment_2
        for n in range(12):
            g = n * 2
            entities = [
                np.array([[float(g + k), float(g + k + 100)]], dtype="float32")
                for k in range(2)
            ]
            build_tbo(tmp_path / f"fragment_{n}.tbo", 2, ["fx"], entities)

        ctx = TboImportContext()
        for n in reversed(range(12)):
            ctx.load_file(str(tmp_path / f"fragment_{n}.tbo"))
        frags = ctx.get_hierarchy().Fragments
        assert len(frags) == 24
        for i, frag in enumerate(frags):
            assert frag.channel("fx")[0] == pytest.approx(float(i))

    def test_keepalive_across_unload(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        frag = h.Fragments[3]
        expected = frag.channel("fx").tolist()

        for i in range(3):
            assert ctx.unload_file(i) is None
        row = np.frombuffer(h.Fragments[3].row(0), dtype="float32").tolist()
        assert row[0] == expected[0]
        assert len(row) == 2  # row covers all channels (fx, fy)

    def test_unload_index_stability(self, tmp_path):
        for n in range(3):
            build_tbo(
                tmp_path / f"fragment_{n}.tbo",
                2,
                ["fx"],
                [np.array([[float(n)]], dtype="float32")],
            )
        ctx = TboImportContext()
        for n in range(3):
            ctx.load_file(str(tmp_path / f"fragment_{n}.tbo"))
        ctx.unload_file(0)
        with pytest.raises(RuntimeError):
            ctx.unload_file(0)
        assert ctx.get_file_info(str(tmp_path / "fragment_1.tbo"))[2] == 1
        h = ctx.get_hierarchy()
        assert h.fragment_count == 2
        assert h.Fragments[0].channel("fx")[0] == pytest.approx(1.0)

    def test_get_file_info(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        version, flags, entity_count, channel_count, channels = ctx.get_file_info(
            str(tmp_path / "fragment_0.tbo")
        )
        assert version == TBO_VERSION
        assert flags == 0
        assert entity_count == 6
        assert channel_count == 2
        assert channels == ["fx", "fy"]
        with pytest.raises(RuntimeError):
            ctx.get_file_info(str(tmp_path / "missing.tbo"))

    def test_unload_by_path(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        ctx.unload_file_by_path(str(tmp_path / "asset_0.tbo"))
        with pytest.raises(RuntimeError):
            ctx.get_file_info(str(tmp_path / "asset_0.tbo"))
        h = ctx.get_hierarchy()
        assert h.asset_count == 0

    def test_iteration_returns_independent_objects(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        frags = h.Fragments
        items = list(frags)
        assert len(items) == 6
        for i, item in enumerate(items):
            assert item is not frags
            assert item.channel("fx")[0] == pytest.approx(float(1 + i))
        for i in range(6):
            a = frags[i]
            b = frags[i]
            assert a is not b
            assert a.channel("fx")[0] == b.channel("fx")[0]

    def test_get_child_returns_none_for_invalid_transition(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        frag = h.Fragments[0]
        assert frag.get_child("Fragments") is None
        assert frag.get_child("Assets") is None
        scene = h.Scenes[0]
        assert scene.get_child("Fragments") is None

    def test_selection_is_relative_for_child_cursor(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        assets = scene.get_child("Assets")
        assert assets is not None
        assets.selected_entity_idx = 1
        assert assets.row_count == 2
        frags = assets.get_child("Fragments")
        assert frags is not None
        assert len(frags) == 2
        frags.selected_entity_idx = 0
        assert frags.channel("fx")[0] == pytest.approx(3.0)
        frags.selected_entity_idx = 1
        assert frags.channel("fx")[0] == pytest.approx(4.0)
        with pytest.raises(IndexError):
            frags.selected_entity_idx = 2

    def test_selected_entity_idx_getter_is_relative(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        assets = scene.get_child("Assets")
        assert assets is not None
        assets.selected_entity_idx = 1
        assert assets.selected_entity_idx == 1
        frags = assets.get_child("Fragments")
        assert frags is not None
        frags.selected_entity_idx = 1
        assert frags.selected_entity_idx == 1

    def test_repr(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        frag = h.Fragments[0]
        r = repr(frag)
        assert "HierarchicalEntity" in r
        assert "idx=0" in r
        assert "rows=4" in r
        assert "channels=2" in r

    def test_points_access(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        frag = h.Fragments[0]
        assert frag.row_count == 4
        points = frag.get_child("Points")
        assert points is not None
        assert len(points) == 4
        for i, pt in enumerate(points):
            assert pt.row_count == 1
            assert pt.channel("fx")[0] == pytest.approx(1.0 + i)
            assert pt.channel("fy")[0] == pytest.approx(101.0 + i)
            row = np.frombuffer(pt.row(0), dtype="float32")
            assert row.tolist() == [1.0 + i, 101.0 + i]
        pt = points[2]
        assert pt.channel("fx")[0] == pytest.approx(3.0)
        with pytest.raises(IndexError):
            points[4]

    def test_points_invalid_transitions(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        assert scene.get_child("Points") is None
        asset = h.Assets[0]
        assert asset.get_child("Points") is None
        frag = h.Fragments[0]
        points = frag.get_child("Points")
        assert points is not None
        pt = points[0]
        assert pt.get_child("Points") is None
        assert pt.get_child("Fragments") is None

    def test_points_parent_access(self, tmp_path):
        make_dataset(tmp_path)
        ctx = TboImportContext()
        load_all(ctx, tmp_path)
        h = ctx.get_hierarchy()
        scene = h.Scenes[0]
        assets = scene.get_child("Assets")
        assert assets is not None
        assets.selected_entity_idx = 1
        frags = assets.get_child("Fragments")
        assert frags is not None
        frags.selected_entity_idx = 0
        points = frags.get_child("Points")
        assert points is not None
        pt = points[0]
        assert pt.fx == pytest.approx(3.0)
        assert pt.fy == pytest.approx(103.0)


def make_fragment(path):
    entities = [np.array([[1.0, 2.0]], dtype="float32") for _ in range(2)]
    build_tbo(path, 2, ["fx", "fy"], entities)


class TestExportContext:
    def test_construction_and_empty_hierarchy(self, tmp_path):
        ctx = TboExportContext(
            str(tmp_path),
            True,   # scene_transform
            False,  # scene_similarity
            True,   # asset_embedding
            True,   # asset_transform
            True,   # fragment_xyz
            False,
            False,
            False,
            1.0,    # max_memory_mb
            1.0,    # target_export_size_mb
            64,     # target_point_count
        )
        assert ctx.accumulate() == []
        h = ctx.get_hierarchy()
        assert h.scene_count == 0
        assert h.asset_count == 0
        assert h.fragment_count == 0

    def test_unique_slab_names(self, tmp_path):
        ctx_a = TboExportContext(str(tmp_path), True, False, False, False, False, False, False, False, 1.0, 1.0, 64)
        ctx_b = TboExportContext(str(tmp_path), True, False, False, False, False, False, False, False, 1.0, 1.0, 64)
        del ctx_a, ctx_b
