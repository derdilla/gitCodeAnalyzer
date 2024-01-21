import git
import shutil
import os
import matplotlib.pyplot as plt
from datetime import datetime
import matplotlib.dates as mdates
import pickle
import numpy as np


def analyze(repo_url, tmp_folder, file_extension):
    # Clone the Git repository to the /tmp folder
    repo = git.Repo.clone_from(repo_url, tmp_folder)

    data = []
    for commit in repo.iter_commits():
        print(f"Commit: {commit.hexsha}")

        # Checkout the commit to get the files at that time
        repo.git.checkout(commit.hexsha, force=True)

        # Get the contents of files with the specified extension
        all_lines = 0
        test_lines = 0
        non_test_lines = 0
        empty_comment_lines_in_test = 0
        empty_comment_lines_in_code = 0
        import_lines_in_code = 0
        for root, dirs, files in os.walk(tmp_folder):
            for file in files:
                if file.endswith(file_extension):
                    file_path = os.path.join(root, file)
                    with open(file_path, 'r', encoding='utf-8') as file_content:
                        content = file_content.read()
                        lines = content.splitlines()
                        all_lines += len(lines)
                        if 'test' in file_path.lower():
                            test_lines += len(lines)
                            empty_comment_lines_in_test += sum(1 for line in lines if line.strip() == '' or line.strip().startswith('//'))
                        else:
                            non_test_lines += len(lines)
                            empty_comment_lines_in_code += sum(1 for line in lines if line.strip() == '' or line.strip().startswith('//'))
                            import_lines_in_code += sum(1 for line in lines if line.strip().startswith('import'))

        data.append((commit.committed_date, all_lines, test_lines, non_test_lines, empty_comment_lines_in_test, empty_comment_lines_in_code, import_lines_in_code))

    # Delete the cloned repository folder
    shutil.rmtree(tmp_folder)

    return data


# noinspection PyTypeChecker
def generate_line_chart(data, smooth=True, smooth_window_size=4):
    timestamps, line_counts, test_lines, non_test_lines, empty_comment_lines_in_test, empty_comment_lines_in_code, import_lines_in_code = zip(*data)

    productive_lines = [a - b - c - d for a, b, c, d in zip(line_counts, test_lines, empty_comment_lines_in_code, import_lines_in_code)]

    dates = [datetime.fromtimestamp(ts) for ts in timestamps]

    if smooth:
        def moving_average(data, window_size):
            return np.convolve(data, np.ones(window_size)/window_size, mode='valid')

        line_counts = moving_average(line_counts, smooth_window_size)
        productive_lines = moving_average(productive_lines, smooth_window_size)
        test_lines = moving_average(test_lines, smooth_window_size)
        empty_comment_lines_in_test = moving_average(empty_comment_lines_in_test, smooth_window_size)
        dates = dates[:len(line_counts)]

    plt.plot(dates, line_counts, marker=None, linestyle='-', color='b', label='Total lines')
    plt.plot(dates, productive_lines, marker=None, linestyle='-', color='g', label='Productive lines')
    plt.plot(dates, test_lines, marker=None, linestyle='-', color='#00f8ff', label='Test lines')
    plt.plot(dates, empty_comment_lines_in_test, marker=None, linestyle='-', color='#00bdff', label='Empty or documentation lines')

    plt.gca().xaxis.set_major_formatter(mdates.DateFormatter('%b-%d'))
    plt.gcf().autofmt_xdate()
    plt.xlabel('Timestamp')
    plt.ylabel('Line count')
    plt.title('Dart line count over time')
    plt.legend()
    plt.grid(True)
    plt.savefig('line_chart.svg')
    plt.show()


def serialize_to_file(data, filename):
    try:
        if os.path.exists(filename):
            os.remove(filename)

        with open(filename, 'wb') as file:
            pickle.dump(data, file)
        print(f'Serialization successful. Data saved to {filename}')
    except (pickle.PickleError, IOError) as e:
        print(f'Error during serialization: {e}')


def deserialize_from_file(filename):
    try:
        with open(filename, 'rb') as file:
            data = pickle.load(file)
        print(f'Deserialization successful. Data loaded from {filename}')
        return data
    except (pickle.PickleError, IOError, FileNotFoundError) as e:
        print(f'Error during deserialization: {e}')
        return None


def main():
    try:
        shutil.rmtree('./tmp')
    except:
        pass

    data = analyze('https://github.com/NobodyForNothing/blood-pressure-monitor-fl', './tmp', '.dart')
    serialize_to_file(data, 'lines_cache.pk1')

    # data = deserialize_from_file('lines_cache.pk1')

    generate_line_chart(data, smooth=False)


if __name__ == '__main__':
    main()

