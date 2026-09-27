let square x = x * x

[<EntryPoint>]
let main argv =
    printfn "%d" (square 4)
    0
