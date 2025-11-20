#!/usr/bin/env bash
# Demo script for Git Stream
# This script simulates an agent making changes to files

echo "🚀 Git Stream Demo"
echo "=================="
echo ""
echo "This script will make changes to files to demonstrate Git Stream."
echo "Make sure Git Stream is running in another terminal:"
echo "  cargo run"
echo ""
read -p "Press Enter to start making changes..."

# Create or modify a test file
echo ""
echo "📝 Creating test_demo.rs..."
cat > test_demo.rs << 'EOF'
// Test file for Git Stream demo
fn main() {
    println!("Hello, Git Stream!");
}
EOF

sleep 2

echo "📝 Adding more code to test_demo.rs..."
cat > test_demo.rs << 'EOF'
// Test file for Git Stream demo
fn main() {
    println!("Hello, Git Stream!");
    println!("Watching changes in real-time!");
    
    let message = "This is pretty cool";
    println!("{}", message);
}
EOF

sleep 2

echo "📝 Modifying README.md..."
echo "" >> README.md
echo "## Demo Run" >> README.md
echo "" >> README.md
echo "This line was added by the demo script at $(date)" >> README.md

sleep 2

echo "📝 Creating another test file..."
cat > test_example.py << 'EOF'
# Python test file
def greet(name):
    print(f"Hello, {name}!")

if __name__ == "__main__":
    greet("Git Stream")
EOF

sleep 2

echo "📝 Modifying test_demo.rs again..."
cat > test_demo.rs << 'EOF'
// Test file for Git Stream demo
// Now with more features!

fn greet(name: &str) {
    println!("Hello, {}!", name);
}

fn main() {
    println!("Hello, Git Stream!");
    println!("Watching changes in real-time!");
    
    let message = "This is pretty cool";
    println!("{}", message);
    
    greet("Developer");
    greet("Git Stream");
}
EOF

echo ""
echo "✅ Demo complete!"
echo "You should have seen several changes in Git Stream."
echo ""
echo "Clean up demo files:"
echo "  rm test_demo.rs test_example.py"
