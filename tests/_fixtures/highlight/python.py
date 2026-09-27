from dataclasses import dataclass


@dataclass
class Point:
    x: int
    y: int

    def norm(self) -> float:
        return (self.x**2 + self.y**2) ** 0.5


print(Point(3, 4).norm())
