# Git code analyzer

Analyze the code from remote git repositories and show how they grew over time.

### Sample 
I used this to create this chart of my [blood pressure monitor](https://github.com/NobodyForNothing/blood-pressure-monitor-fl/) project:
![lines of bp-monitor project](sample_chart.png)

### Usage
To run this for the sample repo install the requirements `pip install -r requirements.txt ` and run the project.

To configure it, change the main method at the bottom of the file. You can:
- change the `repo_url` or the `file_extension` in the call to `analyze` to try other repositories
- remove the `data = analyze...` and `serialize_to_file...` lines and comment out the `data = deserialize...` line in case you already have your data and are only modifying plot generation
