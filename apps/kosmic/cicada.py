"""Cicada cutout, low poly, one closed outline.

Right half only, abdomen tip to head apex; the left half is its mirror. One unit
wide, origin at the middle of the piece. Wound counter-clockwise.

Sized off the 1ft-grid schematic: 6ft wingtip to wingtip, 2.75ft deep.
"""

HALF = [
    (0.000, -0.225),                                        # abdomen tip
    (0.026, -0.196), (0.040, -0.150), (0.048, -0.104),
    (0.055, -0.058), (0.060, -0.020),                       # abdomen
    (0.076, -0.052),                                        # wing root
    (0.150, -0.090), (0.240, -0.100), (0.305, -0.076),      # hind wing
    (0.318, -0.044),
    (0.400, 0.010), (0.462, 0.085), (0.492, 0.148),         # fore wing, trailing
    (0.500, 0.178), (0.486, 0.196), (0.448, 0.194),         # wingtip
    (0.360, 0.174), (0.264, 0.150), (0.170, 0.118),         # fore wing, leading
    (0.106, 0.092),
    (0.074, 0.056), (0.068, 0.104),                         # thorax
    (0.056, 0.140), (0.044, 0.162),                         # head
    (0.152, 0.222), (0.146, 0.234), (0.032, 0.178),         # antenna
    (0.024, 0.190), (0.012, 0.196),
    (0.000, 0.198),                                         # head apex
]

OUTLINE = HALF + [(-x, y) for x, y in reversed(HALF[1:-1])]
