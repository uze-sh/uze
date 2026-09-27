let rec fact n = if n <= 1 then 1 else n * fact (n - 1)

let () = Printf.printf "%d\n" (fact 5)
