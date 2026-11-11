#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# dependencies = ["pillow>=11"]
# ///
"""Draw the Oxycat icon (a stack of three media) into assets/icon.png and assets/icon.ico.

The drawing uses a 180x180 unit grid, rendered at a high resolution and then downscaled, so edges stay smooth.
"""

from pathlib import Path

from PIL import Image, ImageDraw

UNIT = 180
SCALE = 8
ASSETS = Path(__file__).resolve().parent.parent / "assets"
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]

BACKGROUND = "#B9472A"
LED = "#7CF0B0"
# (side, top) colors of each medium, from the bottom of the stack to the top.
MEDIA = [("#7A6F66", "#9A8F86"), ("#C9BFB4", "#E2D9CE"), ("#23263A", "#41466A")]


def draw_icon() -> Image.Image:
    """Draw the icon at UNIT * SCALE pixels.

    Returns:
        The RGBA master image.
    """
    size = UNIT * SCALE
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    def s(value: float) -> int:
        """Convert grid units to master pixels.

        Args:
            value: Length or coordinate in grid units.

        Returns:
            The same value in master pixels.
        """
        return round(value * SCALE)

    def ellipse(cy: float, color: str) -> None:
        """Fill the face of a medium, an ellipse spanning the stack width.

        Args:
            cy: Vertical center in grid units.
            color: Fill color.
        """
        draw.ellipse((s(40), s(cy - 14), s(140), s(cy + 14)), fill=color)

    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=s(36), fill=BACKGROUND)
    for index, (side, top) in enumerate(MEDIA):
        top_y = 122 - 34 * index
        ellipse(top_y + 24, side)
        draw.rectangle((s(40), s(top_y), s(140), s(top_y + 24)), fill=side)
        ellipse(top_y, top)
    draw.ellipse((s(122), s(68), s(130), s(76)), fill=LED)
    return image


def main() -> None:
    """Write assets/icon.png (256 px, window icon) and assets/icon.ico (executable icon)."""
    ASSETS.mkdir(exist_ok=True)
    icon = draw_icon().resize((256, 256), Image.Resampling.LANCZOS)
    icon.save(ASSETS / "icon.png", optimize=True)
    icon.save(ASSETS / "icon.ico", sizes=ICO_SIZES)


if __name__ == "__main__":
    main()
