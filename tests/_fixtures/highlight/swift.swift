import Foundation

struct User {
    let id: Int
    var name: String
}

let user = User(id: 1, name: "ada")
print("Hello, \(user.name)")
