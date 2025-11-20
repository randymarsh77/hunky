#!/usr/bin/env bash
# Demo script for Hunky
# This script simulates an agent making changes to files

echo "🚀 Hunky Demo"
echo "=================="
echo ""
echo "This script will make changes to files to demonstrate Hunky."
echo "Make sure Hunky is running in another terminal:"
echo "  cargo run"
echo ""
read -p "Press Enter to start making changes..."

# Create or modify a test file
echo ""
echo "📝 Creating test_demo.rs..."
cat > test_demo.rs << 'EOF'
// Test file for Hunky demo
fn main() {
    println!("Hello, Hunky!");
}
EOF

sleep 2

echo "📝 Adding more code to test_demo.rs..."
cat > test_demo.rs << 'EOF'
// Test file for Hunky demo
fn main() {
    println!("Hello, Hunky!");
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
    greet("Hunky")
EOF

sleep 2

echo "📝 Modifying test_demo.rs again..."
cat > test_demo.rs << 'EOF'
// Test file for Hunky demo
// Now with more features!

fn greet(name: &str) {
    println!("Hello, {}!", name);
}

fn main() {
    println!("Hello, Hunky!");
    println!("Watching changes in real-time!");
    
    let message = "This is pretty cool";
    println!("{}", message);
    
    greet("Developer");
    greet("Hunky");
}
EOF

echo ""
echo "✅ Demo complete!"
echo "You should have seen several changes in Hunky."
echo ""
echo "Clean up demo files:"
echo "  rm test_demo.rs test_example.py"
