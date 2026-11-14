"""The feature names of the second move model, as `move_policy2.rs` lists
them (`policy2_feature_names`, `unit_feature_names`)."""

MOVE = ["greedy", "greedy_gap", "greedy_top", "act_capture", "act_attack", "act_load",
        "act_join", "act_supply", "act_unload", "act_other", "stay", "move_share",
        "dealt_value", "taken_value", "kill_certain", "kill_possible", "attacker_lost",
        "denial", "target_indirect", "target_hp", "no_counter", "threat_reduction",
        "threat_share", "deferred_share", "threat_value", "defense", "own_property",
        "own_factory", "other_property", "heal", "capture_completes", "capture_hq",
        "capture_factory", "capture_income", "capture_continue", "pull_gain",
        "enemy_distance_change", "enemy_distance", "hq_distance_change",
        "property_distance_change", "friends_adjacent", "enemies_adjacent", "friends_near",
        "indirect_moved", "foot_threat", "lead_threat", "lead_dealt", "late_threat", "guard",
        "unit_hp"]
LOCAL = ["ctx_dist_od", "ctx_friend_d1", "ctx_friend_d2", "ctx_friend_d3", "ctx_moved_d2",
         "ctx_ready_d2", "ctx_enemy_d1", "ctx_enemy_d2", "ctx_enemy_d3", "ctx_enemy_d5",
         "ctx_enemy_hp_d3", "ctx_enemy_indirect_d6", "ctx_last_dest_dist", "ctx_target_attacked",
         "ctx_attacked_adjacent", "ctx_dest_enemy_property", "ctx_dest_neutral_property",
         "ctx_orders_same_dest"]
GROUPS = ["foot", "direct", "indirect", "other"]
REL = ["greedy", "dealt_value", "taken_value", "threat_value", "threat_share", "enemy_distance",
       "enemy_distance_change", "hq_distance_change", "property_distance_change", "defense",
       "friends_near", "friends_adjacent", "enemies_adjacent", "pull_gain", "move_share",
       "threat_reduction"]
TURN = ["d_step", "d_prev_wait", "d_prev_capture", "d_prev_attack", "d_prev_other",
        "d_prev_foot", "d_prev_direct", "d_prev_indirect", "d_prev_othergrp",
        "d_last_none", "d_last_wait", "d_last_capture", "d_last_attack", "d_last_other",
        "d_lastgrp_foot", "d_lastgrp_direct", "d_lastgrp_indirect", "d_lastgrp_other",
        "d_same_kind_prev", "d_day", "d_count",
        "d_units_ready", "d_moved", "d_own_units", "d_enemy_units"]
UNIT_FIELDS = ["u_foot", "u_direct", "u_indirect", "u_other", "u_hp", "u_max_greedy", "u_orders",
               "u_friend_o2", "u_enemy_o3", "u_enemy_o6", "u_last_dest_dist", "u_has_stay",
               "u_nearest_enemy", "u_attacked_adjacent"]
UNIT_REL = ["rel_greedy_gap", "greedy_rank", "greedy_rank_share", "list_pos", "list_pos_share",
            "log_units"]
UNIT_REL_MAX = ["dealt_value", "kill_certain", "capture_completes", "threat_value",
                "enemy_distance", "capture_continue"]
UNIT_TURN = [n for n in TURN if n not in ("d_count", "d_same_kind_prev")]


def order_names():
    names = list(MOVE) + list(LOCAL) + [f"grp_{g}" for g in GROUPS]
    for n in REL:
        names += [f"rel_max_{n}", f"rel_min_{n}", f"rel_mean_{n}"]
    names += ["rel_greedy_rank", "rel_greedy_rank_share", "log_count"]
    names += TURN
    assert len(names) == 148
    return names


def unit_names():
    names = list(UNIT_FIELDS)
    for block in ["max", "best", "stay"]:
        names += [f"{block}_{n}" for n in MOVE]
    names += UNIT_REL + [f"rel_max_{n}" for n in UNIT_REL_MAX] + UNIT_TURN
    assert len(names) == 199
    return names
