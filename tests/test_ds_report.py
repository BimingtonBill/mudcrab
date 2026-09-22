"""tools/ds_report.py: the delegation records summarised.

Every case builds its own CSV in a temporary folder and reads it back through read_csv, so
the fixtures also pin the file format the launcher writes (CRLF, a plain comma layout, no
quoting). Nothing here reads under local/: the report is checked against records the test
makes up, never against the real ones.
"""
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

from tools import ds_report as report

RUNS_HEADER = 'started,label,session,status,turns,tokens_in,tokens_out,seconds,denied'
PILOT_HEADER = ('started,task,type,base,status,scope_ok,checks_passed,checks_total,seconds,'
                'outcome,reviewer,lead_minutes,notes')
STARTED = '2026-09-21T09:00:00'


def run_row(label, status='ok', turns=10, tokens_in=1000, tokens_out=200, seconds=600, denied=0):
    return ','.join((STARTED, label, 'sess-1', status, str(turns), str(tokens_in),
                     str(tokens_out), str(seconds), str(denied)))


def pilot_row(task, kind='implementation', outcome='accepted', reviewer='claude',
              lead_minutes='10', scope_ok='True', checks_passed=2, checks_total=2, seconds=1200,
              status='ok', notes=''):
    return ','.join((STARTED, task, kind, 'c2c40ae', status, scope_ok, str(checks_passed),
                     str(checks_total), str(seconds), outcome, reviewer, str(lead_minutes),
                     notes))


class Fixture(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)

    def write(self, name, header, rows):
        path = self.dir / name
        path.write_text(header + '\r\n' + ''.join(row + '\r\n' for row in rows),
                        encoding='utf-8', newline='')
        return path

    def records(self, pilot_rows, run_rows=()):
        pilot = report.read_csv(self.write('pilot.csv', PILOT_HEADER, pilot_rows))
        runs = report.read_csv(self.write('runs.csv', RUNS_HEADER, run_rows))
        return report.summarise(runs, pilot)

    def cli(self, argv):
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report.main(argv)
        return code, buffer.getvalue()


class ReadCsv(Fixture):
    def test_missing_file_is_empty(self):
        self.assertEqual(report.read_csv(self.dir / 'never-written.csv'), [])

    def test_empty_file_is_empty(self):
        path = self.dir / 'empty.csv'
        path.write_text('', encoding='utf-8')
        self.assertEqual(report.read_csv(path), [])

    def test_header_only_is_empty(self):
        self.assertEqual(report.read_csv(self.write('pilot.csv', PILOT_HEADER, [])), [])

    def test_keys_and_values_are_stripped(self):
        path = self.write('runs.csv', 'started, label ,tokens_in\n',
                          [' 09:00 , i01 , 42 '])
        self.assertEqual(report.read_csv(path), [{'started': '09:00', 'label': 'i01',
                                                  'tokens_in': '42'}])

    def test_a_short_row_is_padded_with_blanks(self):
        path = self.write('pilot.csv', PILOT_HEADER, ['2026-09-21T09:00:00,i01'])
        row = report.read_csv(path)[0]
        self.assertEqual(row['task'], 'i01')
        self.assertEqual(row['lead_minutes'], '')
        self.assertEqual(row['outcome'], '')

    def test_a_blank_line_is_not_a_row_and_a_bom_is_tolerated(self):
        path = self.dir / 'pilot.csv'
        path.write_text('﻿' + PILOT_HEADER + '\r\n\r\n' + pilot_row('i01') + '\r\n',
                        encoding='utf-8', newline='')
        self.assertEqual([row['task'] for row in report.read_csv(path)], ['i01'])


class AcceptanceRate(Fixture):
    """A corrected task is not an accepted one, and an unfinished one is neither."""

    def setUp(self):
        super().setUp()
        self.summary = self.records([
            pilot_row('i01', outcome='accepted'),
            pilot_row('i02', outcome='corrected'),
            pilot_row('i03', outcome='rejected'),
            pilot_row('i04', outcome=''),           # nobody has reviewed it yet
            pilot_row('i05', outcome='pending-review', reviewer=''),
        ])

    def test_decided_counts_only_the_three_verdicts(self):
        impl = self.summary['types']['implementation']
        self.assertEqual(impl['decided'], 3)
        self.assertEqual((impl['accepted'], impl['corrected'], impl['rejected']), (1, 1, 1))
        self.assertEqual(impl['outcome_unrecorded'], 1)
        self.assertEqual(impl['pending'], 1)

    def test_corrected_is_not_in_the_numerator(self):
        impl = self.summary['types']['implementation']
        self.assertAlmostEqual(impl['acceptance_rate'], 1 / 3)
        self.assertEqual(impl['tasks'], 5)

    def test_a_record_with_nothing_decided_has_no_rate(self):
        summary = self.records([pilot_row('i01', outcome='', reviewer='')])
        self.assertIsNone(summary['types']['implementation']['acceptance_rate'])
        self.assertIn('acceptance n/a', report.format_report(summary))

    def test_the_raw_outcomes_are_kept(self):
        self.assertEqual(self.summary['types']['implementation']['outcomes'],
                         {'accepted': 1, 'corrected': 1, 'rejected': 1, 'pending-review': 1})

    def test_an_unrecognised_outcome_is_reported_not_guessed(self):
        summary = self.records([pilot_row('i01', outcome='looks fine'),
                                pilot_row('i02', outcome='accepted')])
        impl = summary['types']['implementation']
        self.assertEqual(impl['unclassified'], 1)
        self.assertEqual(impl['decided'], 1)
        self.assertAlmostEqual(impl['acceptance_rate'], 1.0)
        self.assertEqual(impl['outcomes']['looks fine'], 1)
        self.assertIn('unclassified 1', report.format_report(summary))


class LeadEffortUnknown(Fixture):
    """Unrecorded lead effort is unknown. A report that read it as zero would be a lie."""

    def test_a_type_with_no_recorded_effort_is_unknown_not_zero(self):
        summary = self.records([pilot_row('i01', lead_minutes=''),
                                pilot_row('i02', lead_minutes='')])
        impl = summary['types']['implementation']
        self.assertIsNone(impl['lead_minutes'])
        self.assertEqual(impl['lead_minutes_tasks'], 0)
        self.assertEqual(impl['lead_minutes_unrecorded'], 2)
        self.assertIsNone(impl['lead_minutes_projected'])
        text = report.format_report(summary)
        self.assertIn('unknown: no lead minutes recorded for any of 2 task(s)', text)
        self.assertNotIn('0.0 min recorded', text)

    def test_a_partial_record_is_a_partial_sum_plus_a_marked_projection(self):
        summary = self.records([pilot_row('i01', lead_minutes='30'),
                                pilot_row('i02', lead_minutes='10'),
                                pilot_row('i03', lead_minutes='')])
        impl = summary['types']['implementation']
        self.assertAlmostEqual(impl['lead_minutes'], 40.0)
        self.assertEqual((impl['lead_minutes_tasks'], impl['lead_minutes_unrecorded']), (2, 1))
        self.assertAlmostEqual(impl['lead_minutes_per_task'], 20.0)
        # 20 min/task over three tasks, of which one was never recorded: a projection.
        self.assertAlmostEqual(impl['lead_minutes_projected'], 60.0)
        text = report.format_report(summary)
        self.assertIn('40.0 min recorded for 2 of 3 task(s); 1 unknown', text)
        self.assertIn('~60.0 min projected for all 3', text)

    def test_a_complete_record_needs_no_projection(self):
        summary = self.records([pilot_row('i01', lead_minutes='30'),
                                pilot_row('i02', lead_minutes='10')])
        impl = summary['types']['implementation']
        self.assertAlmostEqual(impl['lead_minutes'], 40.0)
        self.assertIsNone(impl['lead_minutes_projected'])
        lines = report.format_report(summary).splitlines()
        self.assertIn('  lead      40.0 min recorded for 2 of 2 task(s)', lines)
        self.assertTrue(all('~' not in line for line in lines if line.startswith('  lead')))

    def test_the_report_says_unknown_is_not_zero(self):
        summary = self.records([pilot_row('i01', lead_minutes='30'),
                                pilot_row('i02', lead_minutes='')])
        text = report.format_report(summary)
        self.assertIn('never counted as zero', text)
        self.assertIn('unrecorded lead effort is unknown, not zero', text)

    def test_the_overall_total_does_not_absorb_the_gap(self):
        summary = self.records([pilot_row('i01', kind='implementation', lead_minutes='30'),
                                pilot_row('t47', kind='research', lead_minutes='')])
        overall = summary['overall']
        self.assertAlmostEqual(overall['lead_minutes'], 30.0)
        self.assertEqual((overall['lead_minutes_tasks'], overall['lead_minutes_unrecorded']),
                         (1, 1))
        # Reading the unrecorded task as zero would leave no projection and no '1 unknown'.
        self.assertAlmostEqual(overall['lead_minutes_projected'], 60.0)
        self.assertIn('30.0 min recorded for 1 of 2 task(s); 1 unknown; ~60.0 min projected for'
                      ' all 2', report.format_report(summary))

    def test_unrecorded_worker_time_is_unknown_too(self):
        summary = self.records([pilot_row('i01', seconds=''),
                                pilot_row('i02', seconds='600')])
        impl = summary['types']['implementation']
        self.assertAlmostEqual(impl['worker_minutes'], 10.0)
        self.assertEqual(impl['worker_minutes_unrecorded'], 1)
        self.assertIn('worker    10.0 min over 1 of 2 task(s)', report.format_report(summary))
        self.assertIn('worker    unknown: no time recorded', report.format_report(
            self.records([pilot_row('i01', seconds='')])))


class ScopeAndChecks(Fixture):
    def test_a_failed_scope_check_is_counted(self):
        summary = self.records([pilot_row('i01', scope_ok='False'),
                                pilot_row('i02', scope_ok='True')])
        impl = summary['types']['implementation']
        self.assertEqual(impl['scope_failures'], 1)
        self.assertEqual(impl['scope_unrecorded'], 0)

    def test_an_unrecorded_scope_check_is_not_a_pass(self):
        summary = self.records([pilot_row('i01', scope_ok='')])
        impl = summary['types']['implementation']
        self.assertEqual(impl['scope_failures'], 0)
        self.assertEqual(impl['scope_unrecorded'], 1)
        self.assertIn('unknown, not a pass', report.format_report(summary))

    def test_checks_are_summed(self):
        summary = self.records([pilot_row('i01', checks_passed=2, checks_total=2),
                                pilot_row('i02', checks_passed=0, checks_total=1)])
        impl = summary['types']['implementation']
        self.assertEqual((impl['checks_passed'], impl['checks_total']), (2, 3))
        self.assertEqual(impl['checks_unrecorded'], 0)
        self.assertIn('2/3 passed', report.format_report(summary))

    def test_a_task_with_no_checks_recorded_is_not_a_zero_score(self):
        summary = self.records([pilot_row('i01', checks_passed=2, checks_total=2),
                                pilot_row('i02', checks_passed='', checks_total='')])
        impl = summary['types']['implementation']
        self.assertEqual((impl['checks_passed'], impl['checks_total']), (2, 2))
        self.assertEqual(impl['checks_unrecorded'], 1)
        self.assertIn('checks not recorded for 1 task(s)', report.format_report(summary))

    def test_a_number_that_is_not_finite_is_not_a_number(self):
        summary = self.records([pilot_row('i01', lead_minutes='nan', seconds='inf'),
                                pilot_row('i02', lead_minutes='10', seconds='600')])
        impl = summary['types']['implementation']
        self.assertAlmostEqual(impl['lead_minutes'], 10.0)
        self.assertAlmostEqual(impl['worker_minutes'], 10.0)
        self.assertEqual(impl['lead_minutes_unrecorded'], 1)


class RunsAndTokens(Fixture):
    def test_runs_are_matched_to_tasks_by_label(self):
        summary = self.records([pilot_row('i01'), pilot_row('i02')],
                               [run_row('i01'), run_row('i01'), run_row('i02')])
        impl = summary['types']['implementation']
        self.assertEqual(impl['runs'], 3)
        self.assertEqual(impl['run_turns'], 30)
        self.assertEqual(summary['runs'], 3)
        self.assertFalse(summary['notes'])

    def test_a_run_with_no_task_is_kept_in_the_total_and_flagged(self):
        summary = self.records([pilot_row('i01')],
                               [run_row('i01', tokens_in=1000, tokens_out=200),
                                run_row('t47', tokens_in=5000, tokens_out=100)])
        self.assertEqual(summary['runs'], 2)
        self.assertEqual(summary['overall']['runs'], 2)
        self.assertEqual(summary['overall']['unattributed_runs'], 1)
        self.assertEqual(summary['unattributed']['runs'], 1)
        self.assertEqual(summary['unattributed']['tokens'], 5100)
        self.assertEqual(summary['types']['implementation']['runs'], 1)
        self.assertEqual(summary['types']['implementation']['tokens'], 1200)
        # The type blocks and the unattributed runs add up to the overall total, so the
        # research worker's cost is visible without being added to a measured task.
        self.assertEqual(summary['overall']['tokens'],
                         summary['types']['implementation']['tokens']
                         + summary['unattributed']['tokens'])
        self.assertIn('match no task', ' '.join(summary['notes']))
        self.assertIn('1 with no task record', report.format_report(summary))

    def test_tokens_are_summed_from_the_matched_runs(self):
        summary = self.records([pilot_row('i01'), pilot_row('i02')],
                               [run_row('i01', tokens_in=1000, tokens_out=200),
                                run_row('i02', tokens_in=3000, tokens_out=400)])
        impl = summary['types']['implementation']
        self.assertEqual((impl['tokens_in'], impl['tokens_out'], impl['tokens']), (4000, 600, 4600))

    def test_a_task_with_no_run_has_unknown_tokens(self):
        summary = self.records([pilot_row('i01'), pilot_row('i02')], [run_row('i01')])
        impl = summary['types']['implementation']
        self.assertEqual(impl['tasks_without_runs'], 1)
        self.assertIn('1 task(s) have no run record', ' '.join(summary['notes']))
        self.assertIn('no run record', report.format_report(summary))

    def test_a_run_without_tokens_is_unknown_not_zero(self):
        summary = self.records([pilot_row('i01')], [run_row('i01', tokens_in='', tokens_out='')])
        self.assertEqual(summary['types']['implementation']['tokens'], 0)
        self.assertEqual(summary['types']['implementation']['tokens_unrecorded_runs'], 1)
        self.assertIn('record no tokens: unknown, not zero', ' '.join(summary['notes']))

    def test_denied_tool_names_are_counted_and_tallied(self):
        # The launcher writes tool names ("Bash Glob"); older logs wrote a count. Both add up.
        summary = self.records([pilot_row('i01')], [run_row('i01', denied=2),
                                                    run_row('i01', denied='Bash Glob'),
                                                    run_row('i01', denied='Bash'),
                                                    run_row('i01', denied=0)])
        impl = summary['types']['implementation']
        self.assertEqual(impl['denied'], 5)
        self.assertEqual(impl['denied_runs'], 3)
        self.assertEqual(impl['denied_tools'], {'Bash': 2, 'Glob': 1})
        self.assertNotIn('denied_unrecorded_runs', impl)   # names are counted now, so nothing is 'unrecorded'
        self.assertNotIn('denied tools as text', ' '.join(summary['notes']))

    def test_one_run_is_never_counted_for_two_rows_of_the_same_task(self):
        summary = self.records([pilot_row('i01'), pilot_row('i01')], [run_row('i01')])
        self.assertEqual(summary['types']['implementation']['runs'], 1)
        self.assertEqual(summary['overall']['runs'], 1)
        self.assertEqual(summary['types']['implementation']['tasks'], 2)


class Types(Fixture):
    def test_each_type_is_summarised_on_its_own(self):
        summary = self.records([pilot_row('i01', kind='implementation'),
                                pilot_row('i02', kind='implementation', outcome='rejected'),
                                pilot_row('t47', kind='research', outcome='accepted'),
                                pilot_row('t48', kind='review', outcome='')])
        self.assertEqual(sorted(summary['types']), ['implementation', 'research', 'review'])
        self.assertAlmostEqual(summary['types']['implementation']['acceptance_rate'], 0.5)
        self.assertAlmostEqual(summary['types']['research']['acceptance_rate'], 1.0)
        self.assertIsNone(summary['types']['review']['acceptance_rate'])
        self.assertEqual(summary['types']['review']['outcome_unrecorded'], 1)

    def test_a_row_with_no_type_is_not_dropped(self):
        summary = self.records([pilot_row('i01', kind='')])
        self.assertEqual(list(summary['types']), ['(untyped)'])
        self.assertIn('(untyped)', report.format_report(summary))

    def test_one_type_written_two_ways_is_one_block(self):
        # The type is typed by hand, so case and separators drift between rows. Two blocks for
        # one logical type would halve the numbers the type is judged by, and the block name a
        # reader sees must be the key the summary is looked up by.
        summary = self.records([pilot_row('i01', kind='implementation', outcome='accepted'),
                                pilot_row('i02', kind=' Implementation', outcome='rejected'),
                                pilot_row('t47', kind='Data Analysis', outcome='accepted'),
                                pilot_row('t48', kind='data_analysis', outcome='rejected')])
        self.assertEqual(sorted(summary['types']), ['data-analysis', 'implementation'])
        impl = summary['types']['implementation']
        self.assertEqual((impl['tasks'], impl['accepted'], impl['rejected']), (2, 1, 1))
        self.assertAlmostEqual(summary['types']['data-analysis']['acceptance_rate'], 0.5)
        text = report.format_report(summary)
        self.assertIn('data-analysis', text)
        self.assertNotIn('Data Analysis', text)

    def test_the_overall_row_covers_every_task(self):
        summary = self.records([pilot_row('i01', lead_minutes='10'),
                                pilot_row('t47', kind='research', lead_minutes='20')])
        overall = summary['overall']
        self.assertEqual(overall['tasks'], 2)
        self.assertAlmostEqual(overall['lead_minutes'], 30.0)
        self.assertEqual(overall['lead_minutes_unrecorded'], 0)


class MissingOutcomes(Fixture):
    def test_blank_outcome_or_reviewer_is_listed(self):
        rows = [pilot_row('i01'),                       # complete
                pilot_row('i02', outcome=''),           # nobody wrote a verdict
                pilot_row('i03', reviewer=''),          # pending-review, unreviewed
                pilot_row('i04', outcome='', reviewer='')]
        pilot = report.read_csv(self.write('pilot.csv', PILOT_HEADER, rows))
        self.assertEqual(report.missing_outcomes(pilot), ['i02', 'i03', 'i04'])

    def test_a_pending_record_with_a_named_reviewer_is_not_blank(self):
        # 'pending-review' is what the runner writes, so nothing is blank here: this task is
        # unfinished rather than missing. The report shows it under pending, not under todo.
        summary = self.records([pilot_row('i01', outcome='pending-review', reviewer='claude'),
                                pilot_row('i02', outcome='pending-review', reviewer='')])
        self.assertEqual(summary['missing_outcomes'], ['i02'])
        self.assertEqual(summary['types']['implementation']['pending'], 2)

    def test_a_task_recorded_twice_is_listed_once(self):
        pilot = report.read_csv(self.write('pilot.csv', PILOT_HEADER,
                                           [pilot_row('i01', outcome=''), pilot_row('i01')]))
        self.assertEqual(report.missing_outcomes(pilot), ['i01'])

    def test_the_report_ends_with_what_is_still_to_record(self):
        summary = self.records([pilot_row('i01', outcome='')])
        self.assertEqual(summary['missing_outcomes'], ['i01'])
        self.assertIn('outcome or reviewer still blank: i01', report.format_report(summary))


class FormatReport(Fixture):
    def test_one_block_per_type_then_a_total(self):
        summary = self.records([pilot_row('i01', kind='implementation'),
                                pilot_row('t47', kind='research')])
        text = report.format_report(summary)
        lines = text.splitlines()
        self.assertTrue(lines[0].startswith('delegation report: 2 task(s), 0 run(s)'))
        # research sorts after implementation, and both come before the total.
        positions = [text.index('implementation'), text.index('research'), text.index('TOTAL')]
        self.assertEqual(positions, sorted(positions))
        self.assertEqual(len([line for line in lines if line.startswith('  outcome')]), 3)
        self.assertIn('acceptance 1/1 = 100%', text)

    def test_an_empty_record_reports_nothing_rather_than_failing(self):
        summary = self.records([])
        self.assertEqual(summary['tasks'], 0)
        self.assertEqual(summary['runs'], 0)
        self.assertIsNone(summary['overall']['acceptance_rate'])
        self.assertEqual(report.format_report(summary), 'delegation report: nothing recorded yet.')

    def test_runs_without_tasks_still_produce_a_report(self):
        summary = self.records([], [run_row('t47'), run_row('t48')])
        text = report.format_report(summary)
        self.assertIn('2 run(s)', text)
        self.assertIn('nothing decided yet', text)

    def test_tokens_are_shown_in_short_units(self):
        summary = self.records([pilot_row('i01')], [run_row('i01', tokens_in=1_400_000,
                                                            tokens_out=260_000)])
        self.assertIn('1.4M in + 260k out tokens', report.format_report(summary))


class WholeReport(Fixture):
    """The text itself, pinned. It is what the lead reads, so its shape is the contract."""

    def test_a_small_record_renders_exactly_this(self):
        summary = self.records(
            [pilot_row('i01', outcome='accepted', lead_minutes='30', checks_passed=2,
                       checks_total=2, seconds=600),
             pilot_row('i02', outcome='rejected', lead_minutes='', scope_ok='False',
                       checks_passed=1, checks_total=2, seconds=1200),
             pilot_row('t47', kind='research', outcome='pending-review', reviewer='',
                       lead_minutes='15')],
            [run_row('i01', turns=10, tokens_in=1000, tokens_out=200, seconds=600),
             run_row('t47', turns=20, tokens_in=5000, tokens_out=100, seconds=300)])
        text = report.format_report(summary)
        self.assertEqual(text + '\n', EXPECTED_REPORT)  # the golden text ends with a newline


EXPECTED_REPORT = """\
delegation report: 3 task(s), 2 run(s)

implementation   2 task(s), 1 run(s)
  outcome   accepted 1, corrected 0, rejected 1, pending 0, no outcome 0, unclassified 0 - acceptance 1/2 = 50%
  checks    3/4 passed, scope failures 1, scope not recorded 0
  worker    30.0 min over 2 of 2 task(s)
  lead      30.0 min recorded for 1 of 2 task(s); 1 unknown; ~60.0 min projected for all 2
  runs      1 run(s), 10 turns, 0 denied, 1.0k in + 200 out tokens (1 task(s) with no run record)
research         1 task(s), 1 run(s)
  outcome   accepted 0, corrected 0, rejected 0, pending 1, no outcome 0, unclassified 0 - acceptance n/a (nothing decided yet)
  checks    2/2 passed, scope failures 0, scope not recorded 0
  worker    20.0 min over 1 of 1 task(s)
  lead      15.0 min recorded for 1 of 1 task(s)
  runs      1 run(s), 20 turns, 0 denied, 5.0k in + 100 out tokens
TOTAL            3 task(s), 2 run(s)
  outcome   accepted 1, corrected 0, rejected 1, pending 1, no outcome 0, unclassified 0 - acceptance 1/2 = 50%
  checks    5/6 passed, scope failures 1, scope not recorded 0
  worker    50.0 min over 3 of 3 task(s)
  lead      45.0 min recorded for 2 of 3 task(s); 1 unknown; ~67.5 min projected for all 3
  runs      2 run(s), 30 turns, 0 denied, 6.0k in + 300 out tokens (1 task(s) with no run record)
  note      acceptance is accepted / (accepted + corrected + rejected); a
            task still awaiting review is in neither, and corrected is not
            accepted.
  note      unrecorded lead effort is unknown, not zero; a "~" marks a projection from
            the tasks that do have a record, not a measurement.
  note      1 task(s) have no run record: their tokens are unknown
  note      lead effort is recorded for 2 of 3 task(s): the other 1 are unknown, never counted as zero
  todo      outcome or reviewer still blank: t47
"""


class Cli(Fixture):
    def test_prints_the_report_and_exits_zero(self):
        pilot = self.write('pilot.csv', PILOT_HEADER, [pilot_row('i01')])
        runs = self.write('runs.csv', RUNS_HEADER, [run_row('i01')])
        code, out = self.cli(['--runs', str(runs), '--pilot', str(pilot)])
        self.assertEqual(code, 0)
        self.assertIn('delegation report', out)
        self.assertIn('implementation', out)

    def test_json_is_the_same_summary(self):
        pilot = self.write('pilot.csv', PILOT_HEADER, [pilot_row('i01')])
        runs = self.write('runs.csv', RUNS_HEADER, [run_row('i01')])
        code, out = self.cli(['--runs', str(runs), '--pilot', str(pilot), '--json'])
        self.assertEqual(code, 0)
        parsed = json.loads(out)
        self.assertEqual(parsed['tasks'], 1)
        self.assertEqual(parsed['overall']['accepted'], 1)
        self.assertEqual(parsed['types']['implementation']['runs'], 1)
        self.assertIsNone(parsed['types']['implementation']['lead_minutes_projected'])

    def test_missing_records_are_not_an_error(self):
        missing_runs = self.dir / 'no-runs.csv'
        missing_pilot = self.dir / 'no-pilot.csv'
        code, out = self.cli(['--runs', str(missing_runs), '--pilot', str(missing_pilot)])
        self.assertEqual(code, 0)
        self.assertIn('no delegation records yet', out)

    def test_one_missing_record_still_reports_the_other(self):
        pilot = self.write('pilot.csv', PILOT_HEADER, [pilot_row('i01')])
        code, out = self.cli(['--runs', str(self.dir / 'gone.csv'), '--pilot', str(pilot)])
        self.assertEqual(code, 0)
        self.assertIn('implementation', out)
        self.assertIn('have no run record', out)


if __name__ == '__main__':
    unittest.main()
