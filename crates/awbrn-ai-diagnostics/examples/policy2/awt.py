"""Write the tree model files of the second move model (`AWT1`, see
`ModelFile` in `move_policy2.rs`) from a LightGBM model dump."""

import struct


def flatten(dump):
    """The roots, nodes and leaves of a LightGBM `dump_model()` dict, in
    preorder. Only numerical `<=` splits without a missing-value rule are
    accepted: the Rust walk sends a value left when it is at most the
    threshold and has no rule for NaN or zero as missing. The fit rows have
    no NaN, so LightGBM writes the missing type None."""
    roots, nodes, leaves = [], [], []

    def walk(node):
        if "split_index" not in node:
            leaves.append(float(node["leaf_value"]))
            return ~(len(leaves) - 1)
        if node.get("decision_type") != "<=":
            raise ValueError(f"unsupported split {node.get('decision_type')}")
        if node.get("missing_type") != "None":
            raise ValueError(f"unsupported missing type {node.get('missing_type')}")
        index = len(nodes)
        nodes.append([int(node["split_feature"]), float(node["threshold"]), 0, 0])
        left = walk(node["left_child"])
        right = walk(node["right_child"])
        nodes[index][2] = left
        nodes[index][3] = right
        return index

    for tree in dump["tree_info"]:
        roots.append(walk(tree["tree_structure"]))
    return roots, nodes, leaves


def write(path, names, extra, roots, nodes, leaves):
    out = bytearray(b"AWT1")
    out += struct.pack("<I", len(names))
    for name in names:
        raw = name.encode()
        out += struct.pack("<H", len(raw)) + raw
    out += struct.pack("<I", len(extra))
    out += b"".join(struct.pack("<d", float(v)) for v in extra)
    out += struct.pack("<I", len(roots))
    out += b"".join(struct.pack("<i", r) for r in roots)
    out += struct.pack("<I", len(nodes))
    for feature, threshold, left, right in nodes:
        out += struct.pack("<Hdii", feature, threshold, left, right)
    out += struct.pack("<I", len(leaves))
    out += b"".join(struct.pack("<d", v) for v in leaves)
    with open(path, "wb") as h:
        h.write(out)
    return len(out)
