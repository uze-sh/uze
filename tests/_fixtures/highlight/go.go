package main

import "fmt"

type Point struct {
	X, Y int
}

func main() {
	p := Point{X: 1, Y: 2}
	fmt.Printf("%+v\n", p)
}
