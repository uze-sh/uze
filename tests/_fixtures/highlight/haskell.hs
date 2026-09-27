module Main where

data Shape = Circle Double | Square Double

area :: Shape -> Double
area (Circle r) = pi * r * r
area (Square s) = s * s

main :: IO ()
main = print (area (Circle 1.0))
