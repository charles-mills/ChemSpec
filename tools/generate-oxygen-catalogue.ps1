param([string]$Root = (Split-Path -Parent $PSScriptRoot))

$ErrorActionPreference = 'Stop'
$candidateDir = Join-Path $Root 'catalogue/candidates/oxygen-reactions'
$experienceDir = Join-Path $Root 'conformance/end-to-end'
$observationDir = Join-Path $Root 'conformance/observations'
New-Item -ItemType Directory -Force $candidateDir, $experienceDir, $observationDir | Out-Null
function Write-Utf8($path, $content) {
    [System.IO.File]::WriteAllText($path, $content, [System.Text.UTF8Encoding]::new($false))
}

$rulePremise = 'premise.rule.element-oxygen.representative-outcomes'
$structurePremise = 'premise.structure.element-oxygen.structural-models'
$valencePremise = 'premise.valence.element-oxygen.closed-domain'
$observationPremise = 'premise.observation.element-oxygen'
$allPremises = @($rulePremise, $structurePremise, $valencePremise)
$ionRulePremise = 'premise.rule.fixed-charge-ion-pairs'
$ionStructurePremise = 'premise.structure.fixed-charge-ion-pairs'
$ionValencePremise = 'premise.valence.fixed-charge-ion-pairs'
$ionObservationPremise = 'premise.observation.fixed-charge-ion-pairs'
$ionPremises = @($ionRulePremise, $ionStructurePremise, $ionValencePremise)

function Atom($label, $element, [int]$charge, [int]$nonBonding, [int]$unpaired) {
    [ordered]@{ label=$label; element=$element; formal_charge=$charge; non_bonding_electrons=$nonBonding; unpaired_electrons=$unpaired }
}
function Bond($left, $right, $order, $delocalization=$null) {
    $value = [ordered]@{ left=$left; right=$right; order=$order }
    if ($null -ne $delocalization) { $value.delocalization = $delocalization }
    $value
}
function BinaryState($left, $right) { [ordered]@{ left=$left; right=$right } }
function Premise($id, $statement) {
    [ordered]@{ id=$id; statement=$statement; evidence=@('evidence.openstax.oxygen-compounds'); review=[ordered]@{status='provisional';reviewers=@()}; rule_version='1' }
}
function Parameter($category) { [ordered]@{kind='element';category=$category} }
function ParameterValue { [ordered]@{parameter='member'} }
function Exact($structure) { [ordered]@{kind='exact';structure=$structure} }
function TemplateRef($template) { [ordered]@{kind='template';template=$template;arguments=[ordered]@{member=(ParameterValue)}} }
function Role($side, $representation, $coefficient) { [ordered]@{side=$side;representation=$representation;coefficient=$coefficient} }
function Mapping($reactant, $product) { [ordered]@{reactant=$reactant;product=$product;premise_ids=$allPremises} }
function Assignment($atoms, $product) { [ordered]@{kind='assign_product';premise_ids=$allPremises;atoms=$atoms;product=$product} }
function ObservationCompatibility {
    @(
        [ordered]@{subject_role='oxide';predicate='forms';evidence_subject='oxide';premise_id=$observationPremise},
        [ordered]@{subject_role='subject';predicate='disappears';evidence_subject='element';premise_id=$observationPremise}
    )
}

$states = [ordered]@{}
function Add-State($element, [int]$charge, [int]$nonBonding, [int]$unpaired, [int]$bondSum) {
    $key = "$element|$charge|$nonBonding|$unpaired|$bondSum"
    $states[$key] = [ordered]@{element=$element;formal_charge=$charge;non_bonding_electrons=$nonBonding;unpaired_electrons=$unpaired;covalent_bond_order_sum=$bondSum}
}

$elements = [ordered]@{H=1;Li=1;Be=2;B=3;C=4;N=5;O=6;F=7;Na=1;Mg=2;Al=3;Si=4;P=5;S=6;Cl=7;K=1;Ca=2;Sc=3;Ti=4;V=5;Cr=6;Mn=7;Fe=8;Co=9;Ni=10;Cu=11;Zn=12;Br=7;Rb=1;Sr=2;Y=3;Zr=4;Nb=5;Mo=6;Tc=7;Ru=8;Rh=9;Pd=10;Ag=11;Cd=12;I=7;Cs=1;Ba=2;Hf=4;Ta=5;W=6;Re=7;Os=8;Ir=9;Pt=10;Au=11;Hg=12}
$elements.GetEnumerator() | ForEach-Object { }
Add-State O 0 4 0 2; Add-State O 0 5 1 1; Add-State O -1 6 0 1
Add-State O 0 6 2 0; Add-State O -1 7 1 0; Add-State O -2 8 0 0
Add-State H 0 0 0 1; Add-State H 0 1 1 0
Add-State B 0 3 3 0; Add-State B 0 2 2 1; Add-State B 0 1 1 2; Add-State B 0 0 0 3
foreach ($element in @('C','Si')) { Add-State $element 0 4 4 0; Add-State $element 0 2 2 2; Add-State $element 0 0 0 4 }
Add-State S 0 6 4 0; Add-State S 0 4 2 2; Add-State S 0 2 0 4

$categories = @()
$templates = @()
$applications = @()
$patterns = @()
$rules = @()
$structures = @(
    [ordered]@{
        representation='molecular'; id='Oxygen'; premise_id=$structurePremise; formula='O2'
        atoms=@((Atom 'o1' 'O' 0 4 0),(Atom 'o2' 'O' 0 4 0)); bonds=@((Bond 'o1' 'o2' 'double')); groups=@()
    }
)

function Add-Category($id, $members) {
    $script:categories += [ordered]@{id=$id;subject='element';membership=[ordered]@{kind='explicit';members=$members};premise_ids=@($rulePremise)}
}
function Add-MetalFamilyScaffold($name, $category, $members, $charge, $productKind, $metalCount, $oxygenCount) {
    Add-Category $category $members
    $metalTemplate = "Templates.$name`Metal"
    $productTemplate = "Templates.$name`Product"
    $metalPattern = "Patterns.$name`Metal"
    $script:templates += [ordered]@{
        representation='metallic';id=$metalTemplate;parameters=[ordered]@{member=(Parameter $category)}
        sites=@((Atom 'metal' (ParameterValue) $charge 0 0));domains=@([ordered]@{label='metallic';sites=@('metal');delocalized_electrons=$charge});premise_ids=$allPremises
    }
    $script:patterns += [ordered]@{
        id=$metalPattern;variables=[ordered]@{metal=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}}
        relationships=@([ordered]@{kind='metallic_domain';domain='metallic';sites=@('metal');delocalized_electrons=$charge});premise_ids=$allPremises
    }
    foreach ($member in $members) {
        Add-State $member $charge 0 0 0
        for ($remaining=$charge; $remaining -ge 0; $remaining--) { Add-State $member ($charge-$remaining) $remaining $remaining 0 }
        $applicationId = if($member -eq 'Fe'){"Fe$name`MetalForOxygen"}else{"$member`MetalForOxygen"}
        $script:applications += [ordered]@{id=$applicationId;template=$metalTemplate;arguments=[ordered]@{member=$member};formula=$member;premise_ids=$allPremises}
    }

    $components = @()
    for ($m=1; $m -le $metalCount; $m++) {
        $components += [ordered]@{label="metal$m";atoms=@((Atom 'metal' (ParameterValue) $charge 0 0));bonds=@();groups=@()}
    }
    if ($productKind -eq 'normal') {
        for ($o=1; $o -le $oxygenCount; $o++) { $components += [ordered]@{label="oxide$o";atoms=@((Atom 'o' 'O' -2 8 0));bonds=@();groups=@()} }
    } else {
        $deloc = if ($productKind -eq 'superoxide') {[ordered]@{domain='oxygen.resonance';effective_order=[ordered]@{numerator=3;denominator=2}}} else {$null}
        $o1charge = -1; $o2charge = if ($productKind -eq 'superoxide') {0} else {-1}
        $o2nb = if ($productKind -eq 'superoxide') {5} else {6}; $o2u = if ($productKind -eq 'superoxide') {1} else {0}
        $components += [ordered]@{label=$productKind;atoms=@((Atom 'o1' 'O' $o1charge 6 0),(Atom 'o2' 'O' $o2charge $o2nb $o2u));bonds=@((Bond 'o1' 'o2' 'single' $deloc));groups=@()}
    }
    $componentLabels = @($components | ForEach-Object {$_.label})
    $script:templates += [ordered]@{
        representation='ionic';id=$productTemplate;parameters=[ordered]@{member=(Parameter $category)};components=$components
        associations=@([ordered]@{label='ionic';components=$componentLabels});premise_ids=$allPremises
    }
    [ordered]@{metalTemplate=$metalTemplate;productTemplate=$productTemplate;metalPattern=$metalPattern}
}

$oxygenPattern = [ordered]@{
    id='Patterns.Oxygen';variables=[ordered]@{o1=[ordered]@{atom=[ordered]@{element='O'}};o2=[ordered]@{atom=[ordered]@{element='O'}}}
    relationships=@([ordered]@{kind='covalent';bond='oo';left='o1';right='o2';order='double'});premise_ids=$allPremises
}
$patterns += $oxygenPattern

function New-BaseRule($id, $category, $roles, $reactants, $products, $patternsForCase, $mapping, $rewrite) {
    $parameters = if ($null -eq $category) { [ordered]@{} } else { [ordered]@{member=(Parameter $category)} }
    [ordered]@{
        id=$id;parameters=$parameters;roles=$roles;reactants=$reactants
        cases=@([ordered]@{
            status='supported';id='standard';when=[ordered]@{kind='always'};products=$products;patterns=$patternsForCase
            correspondence=$mapping;rewrite=$rewrite;observation_compatibility=(ObservationCompatibility);premise_ids=@($rulePremise)
        })
        applicability=[ordered]@{premise_id=$rulePremise;request_relation='contact';required_context='representative theoretical oxidation outcome selected by the reviewed oxygen catalogue'}
        model_assumptions=[ordered]@{event='representative';sequence='explanatory';premise_ids=@($rulePremise)}
        premise_ids=@('premise.elements.iupac-periodic-table',$rulePremise,$structurePremise,$valencePremise,$observationPremise)
    }
}

function Release-Metal($ref, $charge) {
    [ordered]@{kind='release_metallic';premise_ids=$allPremises;site="$ref.metal";domain="$ref.metallic";allocation='retain_electron';before=[ordered]@{site=@($charge,0,0);domain_electrons=$charge};after=[ordered]@{site=@(0,$charge,$charge);domain_electrons=0}}
}
function Cleave-Oxygen($ref) {
    [ordered]@{kind='cleave_covalent';premise_ids=$allPremises;edge=@("$ref.o1","$ref.o2",'double');allocation='homolytic';before=(BinaryState @(0,4,0) @(0,4,0));after=(BinaryState @(0,6,2) @(0,6,2))}
}
function Change-Oxygen-To-Single($ref) {
    [ordered]@{kind='change_covalent';premise_ids=$allPremises;edge=@("$ref.o1","$ref.o2");old_order='double';new_order='single';allocation='homolytic';before=(BinaryState @(0,4,0) @(0,4,0));after=(BinaryState @(0,5,1) @(0,5,1))}
}

function Add-NormalOxideFamily($name, $category, $members, $charge, $metalPerProduct, $oxygenPerProduct, $metalCoefficient, $oxygenCoefficient, $productCoefficient, $formulas) {
    $scaffold = Add-MetalFamilyScaffold $name $category $members $charge 'normal' $metalPerProduct $oxygenPerProduct
    foreach ($member in $members) { $script:applications += [ordered]@{id="$member$name";template=$scaffold.productTemplate;arguments=[ordered]@{member=$member};formula=$formulas[$member];premise_ids=$allPremises} }
    $roles=[ordered]@{subject=(Role 'reactant' 'metallic' $metalCoefficient);oxygen=(Role 'reactant' 'molecular' $oxygenCoefficient);oxide=(Role 'product' 'ionic' $productCoefficient)}
    $reactants=[ordered]@{subject=(TemplateRef $scaffold.metalTemplate);oxygen=(Exact 'Oxygen')}
    $products=[ordered]@{oxide=(TemplateRef $scaffold.productTemplate)}
    $casePatterns=[ordered]@{subject=$scaffold.metalPattern;oxygen='Patterns.Oxygen'}
    $mapping=@(); $rewrite=@()
    for($m=1;$m -le $metalCoefficient;$m++){ $unit=[math]::Floor(($m-1)/$metalPerProduct)+1; $slot=(($m-1)%$metalPerProduct)+1; $mapping += Mapping "subject[$m].metal" "oxide[$unit].metal$slot.metal"; $rewrite += Release-Metal "subject[$m]" $charge }
    $oxygenAtoms=@(); for($o=1;$o -le $oxygenCoefficient;$o++){ $rewrite += Cleave-Oxygen "oxygen[$o]"; $oxygenAtoms += "oxygen[$o].o1","oxygen[$o].o2" }
    for($i=0;$i -lt $oxygenAtoms.Count;$i++){ $unit=[math]::Floor($i/$oxygenPerProduct)+1; $slot=($i%$oxygenPerProduct)+1; $mapping += Mapping $oxygenAtoms[$i] "oxide[$unit].oxide$slot.o" }
    $donorRemaining=@(); for($m=1;$m -le $metalCoefficient;$m++){$donorRemaining += $charge}; $acceptRemaining=@(); for($o=0;$o -lt $oxygenAtoms.Count;$o++){$acceptRemaining += 2}
    $di=0;$ai=0
    while($di -lt $donorRemaining.Count -and $ai -lt $acceptRemaining.Count){
        $count=[math]::Min($donorRemaining[$di],$acceptRemaining[$ai]); $db=$donorRemaining[$di];$ab=2 - $acceptRemaining[$ai];$da=$db - $count;$aa=$ab + $count
        $rewrite += [ordered]@{kind='transfer_electron';premise_ids=$allPremises;count=$count;donor="subject[$($di + 1)].metal";acceptor=$oxygenAtoms[$ai];before=[ordered]@{donor=@(($charge - $db),$db,$db);acceptor=@(-$ab,(6 + $ab),(2 - $ab))};after=[ordered]@{donor=@(($charge - $da),$da,$da);acceptor=@(-$aa,(6 + $aa),(2 - $aa))}}
        $donorRemaining[$di] -= $count;$acceptRemaining[$ai] -= $count;if($donorRemaining[$di] -eq 0){$di++};if($acceptRemaining[$ai] -eq 0){$ai++}
    }
    for($u=1;$u -le $productCoefficient;$u++){
        $atoms=@();$components=@();$charges=@()
        for($m=1;$m -le $metalPerProduct;$m++){ $idx=(($u-1)*$metalPerProduct)+$m;$atoms += "subject[$idx].metal";$components += ,@("subject[$idx].metal");$charges += $charge }
        for($o=1;$o -le $oxygenPerProduct;$o++){ $idx=(($u-1)*$oxygenPerProduct)+$o;$atom=$oxygenAtoms[$idx-1];$atoms += $atom;$components += ,@($atom);$charges += -2 }
        $rewrite += [ordered]@{kind='associate_ionic';premise_ids=$allPremises;label="ionic.product$u";components=$components;component_charges=$charges}
        $rewrite += Assignment $atoms "oxide[$u]"
    }
    $script:rules += New-BaseRule "Rules.$name" $category $roles $reactants $products $casePatterns $mapping $rewrite
}

function Add-OxygenAnionFamily($name, $category, $members, $charge, $kind, $metalCount, $formulas) {
    $scaffold=Add-MetalFamilyScaffold $name $category $members $charge $kind $metalCount 2
    foreach($member in $members){$script:applications += [ordered]@{id="$member$name";template=$scaffold.productTemplate;arguments=[ordered]@{member=$member};formula=$formulas[$member];premise_ids=$allPremises}}
    $roles=[ordered]@{subject=(Role 'reactant' 'metallic' $metalCount);oxygen=(Role 'reactant' 'molecular' 1);oxide=(Role 'product' 'ionic' 1)}
    $mapping=@();$rewrite=@();for($m=1;$m -le $metalCount;$m++){$mapping+=Mapping "subject[$m].metal" "oxide[1].metal$m.metal";$rewrite+=Release-Metal "subject[$m]" $charge}
    $mapping+=Mapping 'oxygen[1].o1' "oxide[1].$kind.o1"; $mapping+=Mapping 'oxygen[1].o2' "oxide[1].$kind.o2";$rewrite+=Change-Oxygen-To-Single 'oxygen[1]'
    $donorRemaining=@();for($m=1;$m -le $metalCount;$m++){$donorRemaining+=$charge};$oxygenRefs=@('oxygen[1].o1','oxygen[1].o2');$di=0
    $electronTargets=if($kind -eq 'superoxide'){1}else{2}
    for($oi=0;$oi -lt $electronTargets;$oi++){$db=$donorRemaining[$di];$rewrite += [ordered]@{kind='transfer_electron';premise_ids=$allPremises;count=1;donor="subject[$($di + 1)].metal";acceptor=$oxygenRefs[$oi];before=[ordered]@{donor=@(($charge - $db),$db,$db);acceptor=@(0,5,1)};after=[ordered]@{donor=@(($charge - ($db - 1)),($db - 1),($db - 1));acceptor=@(-1,6,0)}};$donorRemaining[$di]--;if($donorRemaining[$di] -eq 0){$di++}}
    if($kind -eq 'superoxide'){$rewrite += [ordered]@{kind='change_covalent_delocalization';premise_ids=$allPremises;edge=@('oxygen[1].o1','oxygen[1].o2');expected=$null;replacement=[ordered]@{domain='oxygen.resonance';effective_order=[ordered]@{numerator=3;denominator=2}}}}
    $components=@();$charges=@();$atoms=@();for($m=1;$m -le $metalCount;$m++){$components+=,@("subject[$m].metal");$charges+=$charge;$atoms+="subject[$m].metal"};$components+=,@('oxygen[1].o1','oxygen[1].o2');$charges+=(-1*$charge*$metalCount);$atoms+='oxygen[1].o1','oxygen[1].o2'
    $rewrite += [ordered]@{kind='associate_ionic';premise_ids=$allPremises;label='ionic.product1';components=$components;component_charges=$charges};$rewrite+=Assignment $atoms 'oxide[1]'
    $script:rules += New-BaseRule "Rules.$name" $category $roles ([ordered]@{subject=(TemplateRef $scaffold.metalTemplate);oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef $scaffold.productTemplate)}) ([ordered]@{subject=$scaffold.metalPattern;oxygen='Patterns.Oxygen'}) $mapping $rewrite
}

function Add-CovalentDioxideFamily($name,$category,$members,$initialNonBonding,$formulas){
    Add-Category $category $members;$subjectTemplate="Templates.$name`Element";$productTemplate="Templates.$name`Dioxide";$subjectPattern="Patterns.$name`Element"
    $script:templates += [ordered]@{representation='molecular';id=$subjectTemplate;parameters=[ordered]@{member=(Parameter $category)};atoms=@((Atom 'x' (ParameterValue) 0 $initialNonBonding 4));bonds=@();groups=@();premise_ids=$allPremises}
    $script:templates += [ordered]@{representation='molecular';id=$productTemplate;parameters=[ordered]@{member=(Parameter $category)};atoms=@((Atom 'x' (ParameterValue) 0 ($initialNonBonding - 4) 0),(Atom 'o1' 'O' 0 4 0),(Atom 'o2' 'O' 0 4 0));bonds=@((Bond 'x' 'o1' 'double'),(Bond 'x' 'o2' 'double'));groups=@();premise_ids=$allPremises}
    $script:patterns += [ordered]@{id=$subjectPattern;variables=[ordered]@{x=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}};relationships=@();premise_ids=$allPremises}
    foreach($member in $members){$script:applications += [ordered]@{id="$member`ForOxygen";template=$subjectTemplate;arguments=[ordered]@{member=$member};formula=$member;premise_ids=$allPremises};$script:applications += [ordered]@{id="$member$name";template=$productTemplate;arguments=[ordered]@{member=$member};formula=$formulas[$member];premise_ids=$allPremises}}
    $mapping=@((Mapping 'subject[1].x' 'oxide[1].x'),(Mapping 'oxygen[1].o1' 'oxide[1].o1'),(Mapping 'oxygen[1].o2' 'oxide[1].o2'))
    $rewrite=@((Cleave-Oxygen 'oxygen[1]'))
    $xBefore=$initialNonBonding
    foreach($o in @('o1','o2')){$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@('subject[1].x',"oxygen[1].$o",'double');electron_contribution=[ordered]@{left=2;right=2};before=(BinaryState @(0,$xBefore,($xBefore - ($initialNonBonding - 4))) @(0,6,2));after=(BinaryState @(0,($xBefore - 2),[math]::Max(0,(($xBefore - ($initialNonBonding - 4)) - 2))) @(0,4,0))};$xBefore -= 2}
    $rewrite += Assignment @('subject[1].x','oxygen[1].o1','oxygen[1].o2') 'oxide[1]'
    $script:rules += New-BaseRule "Rules.$name" $category ([ordered]@{subject=(Role 'reactant' 'molecular' 1);oxygen=(Role 'reactant' 'molecular' 1);oxide=(Role 'product' 'molecular' 1)}) ([ordered]@{subject=(TemplateRef $subjectTemplate);oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef $productTemplate)}) ([ordered]@{subject=$subjectPattern;oxygen='Patterns.Oxygen'}) $mapping $rewrite
}

Add-NormalOxideFamily 'MonovalentNormalOxide' 'Categories.MonovalentNormalOxideMetal' @('Li') 1 2 1 4 1 2 ([ordered]@{Li='Li2O'})
Add-NormalOxideFamily 'DivalentNormalOxide' 'Categories.DivalentNormalOxideMetal' @('Be','Mg','Ca','Sr','Ba') 2 1 1 2 1 2 ([ordered]@{Be='BeO';Mg='MgO';Ca='CaO';Sr='SrO';Ba='BaO'})
Add-NormalOxideFamily 'TrivalentNormalOxide' 'Categories.TrivalentNormalOxideMetal' @('Al') 3 2 3 4 3 2 ([ordered]@{Al='Al2O3'})
Add-OxygenAnionFamily 'MonovalentPeroxide' 'Categories.MonovalentPeroxideMetal' @('Na') 1 'peroxide' 2 ([ordered]@{Na='Na2O2'})
Add-OxygenAnionFamily 'Superoxide' 'Categories.SuperoxideMetal' @('K','Rb','Cs') 1 'superoxide' 1 ([ordered]@{K='KO2';Rb='RbO2';Cs='CsO2'})
Add-CovalentDioxideFamily 'Group14Dioxide' 'Categories.Group14DioxideElement' @('C','Si') 4 ([ordered]@{C='CO2';Si='SiO2'})
Add-CovalentDioxideFamily 'SulfurDioxide' 'Categories.SulfurDioxideElement' @('S') 6 ([ordered]@{S='SO2'})

# Transition-metal oxidation is encoded by periodic-group source families and
# oxide-stoichiometry product families. A structure application is still made
# for each selected element, but neither the reaction rule nor its electron
# process is authored as an element-specific experience.
$transitionExperiences = @()
$transitionMetallicStates = @()
$transitionAtomicNumbers = [ordered]@{Sc=21;Ti=22;V=23;Cr=24;Mn=25;Fe=26;Co=27;Ni=28;Cu=29;Zn=30;Y=39;Zr=40;Nb=41;Mo=42;Tc=43;Ru=44;Rh=45;Pd=46;Ag=47;Cd=48;Hf=72;Ta=73;W=74;Re=75;Os=76;Ir=77;Pt=78;Au=79;Hg=80}
function Get-HundUnpaired([int]$localElectrons) {
    if ($localElectrons -le 5) { return $localElectrons }
    if ($localElectrons -le 10) { return 10 - $localElectrons }
    return 0
}
function Get-OxideFormula($member, [int]$metalCount, [int]$oxygenCount) {
    $m = if ($metalCount -eq 1) { '' } else { "$metalCount" }
    $o = if ($oxygenCount -eq 1) { 'O' } else { "O$oxygenCount" }
    "$member$m$o"
}
function Get-TransitionSlug($member, $oxidations, [int]$oxygenCount) {
    $oxidationKey = ($oxidations | ForEach-Object { "$_" }) -join '-'
    "$($member.ToLowerInvariant())-oxide-$oxidationKey-o$oxygenCount"
}
function Add-TransitionOxideFamily($name, $members, [int]$domainElectrons, $oxidations, [int]$oxygenPerProduct) {
    $category = "Categories.$name`Element"
    $metalTemplate = "Templates.$name`Metal"
    $productTemplate = "Templates.$name`Oxide"
    $metalPattern = "Patterns.$name`Metal"
    $metalPerProduct = $oxidations.Count
    $productCoefficient = if (($oxygenPerProduct % 2) -eq 0) { 1 } else { 2 }
    $metalCoefficient = $metalPerProduct * $productCoefficient
    $oxygenCoefficient = [int](($oxygenPerProduct * $productCoefficient) / 2)
    $expandedOxidations = @(); for ($u=1; $u -le $productCoefficient; $u++) { $expandedOxidations += $oxidations }

    Add-Category $category $members
    $script:templates += [ordered]@{representation='metallic';id=$metalTemplate;parameters=[ordered]@{member=(Parameter $category)};sites=@((Atom 'metal' (ParameterValue) $domainElectrons 0 0));domains=@([ordered]@{label='metallic';sites=@('metal');delocalized_electrons=$domainElectrons});premise_ids=$allPremises}
    $script:patterns += [ordered]@{id=$metalPattern;variables=[ordered]@{metal=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}};relationships=@([ordered]@{kind='metallic_domain';domain='metallic';sites=@('metal');delocalized_electrons=$domainElectrons});premise_ids=$allPremises}

    $components=@()
    for($m=1;$m -le $metalPerProduct;$m++){
        $charge=[int]$oxidations[$m-1];$local=$domainElectrons-$charge;$unpaired=Get-HundUnpaired $local
        $components += [ordered]@{label="metal$m";atoms=@((Atom 'metal' (ParameterValue) $charge $local $unpaired));bonds=@();groups=@()}
    }
    for($o=1;$o -le $oxygenPerProduct;$o++){$components += [ordered]@{label="oxide$o";atoms=@((Atom 'o' 'O' -2 8 0));bonds=@();groups=@()}}
    $componentLabels=@($components|ForEach-Object{$_.label})
    $script:templates += [ordered]@{representation='ionic';id=$productTemplate;parameters=[ordered]@{member=(Parameter $category)};components=$components;associations=@([ordered]@{label='ionic';components=$componentLabels});premise_ids=$allPremises}

    foreach($member in $members){
        $sourceId="$member$name`MetalForOxygen";$productId="$member$name`Oxide";$formula=Get-OxideFormula $member $metalPerProduct $oxygenPerProduct
        $script:applications += [ordered]@{id=$sourceId;template=$metalTemplate;arguments=[ordered]@{member=$member};formula=$member;premise_ids=$allPremises}
        $script:applications += [ordered]@{id=$productId;template=$productTemplate;arguments=[ordered]@{member=$member};formula=$formula;premise_ids=$allPremises}
        Add-State $member $domainElectrons 0 0 0
        Add-State $member 0 $domainElectrons $domainElectrons 0
        for($charge=1;$charge -le ($oxidations|Measure-Object -Maximum).Maximum;$charge++){Add-State $member $charge ($domainElectrons-$charge) ($domainElectrons-$charge) 0}
        foreach($charge in $oxidations){$local=$domainElectrons-$charge;Add-State $member $charge $local (Get-HundUnpaired $local) 0}
        if(-not ($script:transitionMetallicStates|Where-Object{$_.element -eq $member})){$script:transitionMetallicStates += [ordered]@{element=$member;site_formal_charge=$domainElectrons;site_local_electrons=0;delocalized_electrons_per_site=$domainElectrons}}
        $formulaLeft=if($metalCoefficient -eq 1){$member}else{"$metalCoefficient $member"};$oxygenLeft=if($oxygenCoefficient -eq 1){'O2'}else{"$oxygenCoefficient O2"};$formulaRight=if($productCoefficient -eq 1){$formula}else{"$productCoefficient $formula"}
        $equation="$formulaLeft + $oxygenLeft -> $formulaRight";$slug=Get-TransitionSlug $member $oxidations $oxygenPerProduct
        $script:transitionExperiences += ,@($slug,"$member`AndOxygenTo$name","$metalCoefficient",$sourceId,$member,'metallic',"$productCoefficient",$productId,$formula,'ionic',$equation,"Rules.$name",[int]$transitionAtomicNumbers[$member])
    }

    $mapping=@();$rewrite=@()
    for($m=1;$m -le $metalCoefficient;$m++){$unit=[math]::Floor(($m-1)/$metalPerProduct)+1;$slot=(($m-1)%$metalPerProduct)+1;$mapping+=Mapping "subject[$m].metal" "oxide[$unit].metal$slot.metal";$rewrite += [ordered]@{kind='release_metallic';premise_ids=$allPremises;site="subject[$m].metal";domain="subject[$m].metallic";allocation='retain_electron';before=[ordered]@{site=@($domainElectrons,0,0);domain_electrons=$domainElectrons};after=[ordered]@{site=@(0,$domainElectrons,$domainElectrons);domain_electrons=0}}}
    $oxygenAtoms=@();for($o=1;$o -le $oxygenCoefficient;$o++){$rewrite+=Cleave-Oxygen "oxygen[$o]";$oxygenAtoms+="oxygen[$o].o1","oxygen[$o].o2"}
    for($i=0;$i -lt $oxygenAtoms.Count;$i++){$unit=[math]::Floor($i/$oxygenPerProduct)+1;$slot=($i%$oxygenPerProduct)+1;$mapping+=Mapping $oxygenAtoms[$i] "oxide[$unit].oxide$slot.o"}
    $donors=@();for($m=1;$m -le $metalCoefficient;$m++){$donors += [ordered]@{ref="subject[$m].metal";target=[int]$expandedOxidations[$m-1];sent=0}}
    $di=0;for($oi=0;$oi -lt $oxygenAtoms.Count;$oi++){$received=0;while($received -lt 2){$donor=$donors[$di];$remaining=$donor.target-$donor.sent;$count=[math]::Min($remaining,(2-$received));$beforeSent=$donor.sent;$afterSent=$beforeSent+$count;$rewrite += [ordered]@{kind='transfer_electron';premise_ids=$allPremises;count=$count;donor=$donor.ref;acceptor=$oxygenAtoms[$oi];before=[ordered]@{donor=@($beforeSent,($domainElectrons-$beforeSent),($domainElectrons-$beforeSent));acceptor=@(-$received,(6+$received),(2-$received))};after=[ordered]@{donor=@($afterSent,($domainElectrons-$afterSent),($domainElectrons-$afterSent));acceptor=@(-($received+$count),(6+$received+$count),(2-$received-$count))}};$donor.sent=$afterSent;$received+=$count;if($donor.sent -eq $donor.target){$local=$domainElectrons-$donor.target;$hund=Get-HundUnpaired $local;if($hund -ne $local){$rewrite += [ordered]@{kind='reconfigure_electrons';premise_ids=$allPremises;atom=$donor.ref;before=@($donor.target,$local,$local);after=@($donor.target,$local,$hund)}};$di++}}}
    for($u=1;$u -le $productCoefficient;$u++){$atoms=@();$groups=@();$charges=@();for($m=1;$m -le $metalPerProduct;$m++){$idx=(($u-1)*$metalPerProduct)+$m;$charge=[int]$expandedOxidations[$idx-1];$atoms+="subject[$idx].metal";$groups+=,@("subject[$idx].metal");$charges+=$charge};for($o=1;$o -le $oxygenPerProduct;$o++){$idx=(($u-1)*$oxygenPerProduct)+$o;$atom=$oxygenAtoms[$idx-1];$atoms+=$atom;$groups+=,@($atom);$charges+=-2};$rewrite += [ordered]@{kind='associate_ionic';premise_ids=$allPremises;label="ionic.product$u";components=$groups;component_charges=$charges};$rewrite+=Assignment $atoms "oxide[$u]"}
    $script:rules += New-BaseRule "Rules.$name" $category ([ordered]@{subject=(Role 'reactant' 'metallic' $metalCoefficient);oxygen=(Role 'reactant' 'molecular' $oxygenCoefficient);oxide=(Role 'product' 'ionic' $productCoefficient)}) ([ordered]@{subject=(TemplateRef $metalTemplate);oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef $productTemplate)}) ([ordered]@{subject=$metalPattern;oxygen='Patterns.Oxygen'}) $mapping $rewrite
}

function Add-TransitionCovalentOxideFamily($name, $members, [int]$domainElectrons, [int]$metalPerProduct, [int]$oxygenPerProduct) {
    $category="Categories.$name`Element";$metalTemplate="Templates.$name`Metal";$productTemplate="Templates.$name`Oxide";$metalPattern="Patterns.$name`Metal"
    $productCoefficient=if(($oxygenPerProduct%2)-eq 0){1}else{2};$metalCoefficient=$metalPerProduct*$productCoefficient;$oxygenCoefficient=[int](($oxygenPerProduct*$productCoefficient)/2)
    Add-Category $category $members
    $script:templates += [ordered]@{representation='metallic';id=$metalTemplate;parameters=[ordered]@{member=(Parameter $category)};sites=@((Atom 'metal' (ParameterValue) $domainElectrons 0 0));domains=@([ordered]@{label='metallic';sites=@('metal');delocalized_electrons=$domainElectrons});premise_ids=$allPremises}
    $script:patterns += [ordered]@{id=$metalPattern;variables=[ordered]@{metal=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}};relationships=@([ordered]@{kind='metallic_domain';domain='metallic';sites=@('metal');delocalized_electrons=$domainElectrons});premise_ids=$allPremises}
    $productAtoms=@();for($m=1;$m-le$metalPerProduct;$m++){$productAtoms+=Atom "metal$m" (ParameterValue) 0 0 0};for($o=1;$o-le$oxygenPerProduct;$o++){$productAtoms+=Atom "o$o" 'O' 0 4 0}
    $productBonds=@()
    if($metalPerProduct-eq1){for($o=1;$o-le$oxygenPerProduct;$o++){$productBonds+=Bond 'metal1' "o$o" 'double'}}
    else{for($m=1;$m-le$metalPerProduct;$m++){for($slot=1;$slot-le3;$slot++){$o=(($m-1)*3)+$slot;$productBonds+=Bond "metal$m" "o$o" 'double'}};$bridge=$oxygenPerProduct;$productBonds+=Bond 'metal1' "o$bridge" 'single';$productBonds+=Bond 'metal2' "o$bridge" 'single'}
    $script:templates += [ordered]@{representation='molecular';id=$productTemplate;parameters=[ordered]@{member=(Parameter $category)};atoms=$productAtoms;bonds=$productBonds;groups=@();premise_ids=$allPremises}
    foreach($member in $members){
        $sourceId="$member$name`MetalForOxygen";$productId="$member$name`Oxide";$formula=Get-OxideFormula $member $metalPerProduct $oxygenPerProduct
        $script:applications += [ordered]@{id=$sourceId;template=$metalTemplate;arguments=[ordered]@{member=$member};formula=$member;premise_ids=$allPremises},[ordered]@{id=$productId;template=$productTemplate;arguments=[ordered]@{member=$member};formula=$formula;premise_ids=$allPremises}
        Add-State $member $domainElectrons 0 0 0
        for($remaining=$domainElectrons;$remaining-ge1;$remaining-=2){Add-State $member 0 $remaining $remaining ($domainElectrons-$remaining)}
        Add-State $member 0 0 0 $domainElectrons
        if(-not($script:transitionMetallicStates|Where-Object{$_.element-eq$member})){$script:transitionMetallicStates += [ordered]@{element=$member;site_formal_charge=$domainElectrons;site_local_electrons=0;delocalized_electrons_per_site=$domainElectrons}}
        $oxidations=@();for($m=1;$m-le$metalPerProduct;$m++){$oxidations+=[int](2*$oxygenPerProduct/$metalPerProduct)};$slug=Get-TransitionSlug $member $oxidations $oxygenPerProduct
        $left=if($metalCoefficient-eq1){$member}else{"$metalCoefficient $member"};$oLeft=if($oxygenCoefficient-eq1){'O2'}else{"$oxygenCoefficient O2"};$right=if($productCoefficient-eq1){$formula}else{"$productCoefficient $formula"};$equation="$left + $oLeft -> $right"
        $script:transitionExperiences += ,@($slug,"$member`AndOxygenTo$name","$metalCoefficient",$sourceId,$member,'metallic',"$productCoefficient",$productId,$formula,'molecular',$equation,"Rules.$name",[int]$transitionAtomicNumbers[$member])
    }
    $mapping=@();$rewrite=@();for($m=1;$m-le$metalCoefficient;$m++){$unit=[math]::Floor(($m-1)/$metalPerProduct)+1;$slot=(($m-1)%$metalPerProduct)+1;$mapping+=Mapping "subject[$m].metal" "oxide[$unit].metal$slot";$rewrite += [ordered]@{kind='release_metallic';premise_ids=$allPremises;site="subject[$m].metal";domain="subject[$m].metallic";allocation='retain_electron';before=[ordered]@{site=@($domainElectrons,0,0);domain_electrons=$domainElectrons};after=[ordered]@{site=@(0,$domainElectrons,$domainElectrons);domain_electrons=0}}}
    $oxygenAtoms=@();for($o=1;$o-le$oxygenCoefficient;$o++){$rewrite+=Cleave-Oxygen "oxygen[$o]";$oxygenAtoms+="oxygen[$o].o1","oxygen[$o].o2"};for($i=0;$i-lt$oxygenAtoms.Count;$i++){$unit=[math]::Floor($i/$oxygenPerProduct)+1;$slot=($i%$oxygenPerProduct)+1;$mapping+=Mapping $oxygenAtoms[$i] "oxide[$unit].o$slot"}
    for($u=1;$u-le$productCoefficient;$u++){
        $metalRefs=@();for($m=1;$m-le$metalPerProduct;$m++){$idx=(($u-1)*$metalPerProduct)+$m;$metalRefs+="subject[$idx].metal"};$unitOxygen=$oxygenAtoms[(($u-1)*$oxygenPerProduct)..(($u*$oxygenPerProduct)-1)]
        if($metalPerProduct-eq1){$remaining=$domainElectrons;foreach($oxygen in $unitOxygen){$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@($metalRefs[0],$oxygen,'double');electron_contribution=[ordered]@{left=2;right=2};before=(BinaryState @(0,$remaining,$remaining) @(0,6,2));after=(BinaryState @(0,($remaining-2),($remaining-2)) @(0,4,0))};$remaining-=2}}
        else{for($m=0;$m-lt2;$m++){$remaining=$domainElectrons;for($slot=0;$slot-lt3;$slot++){$oxygen=$unitOxygen[($m*3)+$slot];$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@($metalRefs[$m],$oxygen,'double');electron_contribution=[ordered]@{left=2;right=2};before=(BinaryState @(0,$remaining,$remaining) @(0,6,2));after=(BinaryState @(0,($remaining-2),($remaining-2)) @(0,4,0))};$remaining-=2}};$bridge=$unitOxygen[$oxygenPerProduct-1];$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@($metalRefs[0],$bridge,'single');electron_contribution=[ordered]@{left=1;right=1};before=(BinaryState @(0,1,1) @(0,6,2));after=(BinaryState @(0,0,0) @(0,5,1))};$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@($metalRefs[1],$bridge,'single');electron_contribution=[ordered]@{left=1;right=1};before=(BinaryState @(0,1,1) @(0,5,1));after=(BinaryState @(0,0,0) @(0,4,0))}}
        $rewrite+=Assignment ($metalRefs+$unitOxygen) "oxide[$u]"
    }
    $script:rules += New-BaseRule "Rules.$name" $category ([ordered]@{subject=(Role 'reactant' 'metallic' $metalCoefficient);oxygen=(Role 'reactant' 'molecular' $oxygenCoefficient);oxide=(Role 'product' 'molecular' $productCoefficient)}) ([ordered]@{subject=(TemplateRef $metalTemplate);oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef $productTemplate)}) ([ordered]@{subject=$metalPattern;oxygen='Patterns.Oxygen'}) $mapping $rewrite
}

# Each call is a family: members share the same metallic electron pool and the
# same oxide stoichiometry/oxidation-state process. Multiple calls for a member
# become selectable product outcomes in the app.
Add-TransitionOxideFamily 'TransitionG3Sesquioxide' @('Sc','Y') 3 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG4Monoxide' @('Ti') 4 @(2) 1
Add-TransitionOxideFamily 'TransitionG4Sesquioxide' @('Ti') 4 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG4Dioxide' @('Ti','Zr','Hf') 4 @(4) 2
Add-TransitionOxideFamily 'TransitionG5Monoxide' @('V') 5 @(2) 1
Add-TransitionOxideFamily 'TransitionG5Sesquioxide' @('V') 5 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG5Dioxide' @('V','Nb') 5 @(4) 2
Add-TransitionOxideFamily 'TransitionG5Pentoxide' @('V','Nb','Ta') 5 @(5,5) 5
Add-TransitionOxideFamily 'TransitionG6Monoxide' @('Cr') 6 @(2) 1
Add-TransitionOxideFamily 'TransitionG6Sesquioxide' @('Cr') 6 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG6Dioxide' @('Cr','Mo','W') 6 @(4) 2
Add-TransitionOxideFamily 'TransitionG7Monoxide' @('Mn') 7 @(2) 1
Add-TransitionOxideFamily 'TransitionG7Sesquioxide' @('Mn') 7 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG7Dioxide' @('Mn','Tc','Re') 7 @(4) 2
Add-TransitionOxideFamily 'TransitionG7MixedOxide' @('Mn') 7 @(2,3,3) 4
Add-TransitionOxideFamily 'TransitionG8Monoxide' @('Fe') 8 @(2) 1
Add-TransitionOxideFamily 'TransitionG8Sesquioxide' @('Fe') 8 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG8MixedOxide' @('Fe') 8 @(2,3,3) 4
Add-TransitionOxideFamily 'TransitionG8Dioxide' @('Ru','Os') 8 @(4) 2
Add-TransitionOxideFamily 'TransitionG9Monoxide' @('Co') 9 @(2) 1
Add-TransitionOxideFamily 'TransitionG9Sesquioxide' @('Co','Rh') 9 @(3,3) 3
Add-TransitionOxideFamily 'TransitionG9MixedOxide' @('Co') 9 @(2,3,3) 4
Add-TransitionOxideFamily 'TransitionG9Dioxide' @('Rh','Ir') 9 @(4) 2
Add-TransitionOxideFamily 'TransitionG10Monoxide' @('Ni','Pd') 10 @(2) 1
Add-TransitionOxideFamily 'TransitionG11Hemioxide' @('Cu') 11 @(1,1) 1
Add-TransitionOxideFamily 'TransitionG11Monoxide' @('Cu') 11 @(2) 1
Add-TransitionOxideFamily 'TransitionG12Monoxide' @('Zn','Cd','Hg') 12 @(2) 1
Add-TransitionCovalentOxideFamily 'TransitionG6Trioxide' @('Cr','Mo','W') 6 1 3
Add-TransitionCovalentOxideFamily 'TransitionG7Heptoxide' @('Mn','Tc','Re') 7 2 7
Add-TransitionCovalentOxideFamily 'TransitionG8Tetroxide' @('Ru','Os') 8 1 4

# Boron oxide is a five-atom representative network fragment with three bridging oxygens.
Add-Category 'Categories.BoronOxideElement' @('B')
$templates += [ordered]@{representation='molecular';id='Templates.BoronElement';parameters=[ordered]@{member=(Parameter 'Categories.BoronOxideElement')};atoms=@((Atom 'b' (ParameterValue) 0 3 3));bonds=@();groups=@();premise_ids=$allPremises}
$templates += [ordered]@{representation='molecular';id='Templates.BoronOxide';parameters=[ordered]@{member=(Parameter 'Categories.BoronOxideElement')};atoms=@((Atom 'b1' (ParameterValue) 0 0 0),(Atom 'b2' (ParameterValue) 0 0 0),(Atom 'o1' 'O' 0 4 0),(Atom 'o2' 'O' 0 4 0),(Atom 'o3' 'O' 0 4 0));bonds=@((Bond 'b1' 'o1' 'single'),(Bond 'b1' 'o2' 'single'),(Bond 'b1' 'o3' 'single'),(Bond 'b2' 'o1' 'single'),(Bond 'b2' 'o2' 'single'),(Bond 'b2' 'o3' 'single'));groups=@();premise_ids=$allPremises}
$patterns += [ordered]@{id='Patterns.BoronElement';variables=[ordered]@{b=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}};relationships=@();premise_ids=$allPremises}
$applications += [ordered]@{id='BForOxygen';template='Templates.BoronElement';arguments=[ordered]@{member='B'};formula='B';premise_ids=$allPremises},[ordered]@{id='BBoronOxide';template='Templates.BoronOxide';arguments=[ordered]@{member='B'};formula='B2O3';premise_ids=$allPremises}
$mapping=@();$rewrite=@();for($b=1;$b -le 4;$b++){$unit=[math]::Floor(($b-1)/2)+1;$slot=(($b-1)%2)+1;$mapping+=Mapping "subject[$b].b" "oxide[$unit].b$slot"}
$oxygenAtoms=@();for($o=1;$o -le 3;$o++){$rewrite+=Cleave-Oxygen "oxygen[$o]";$oxygenAtoms+="oxygen[$o].o1","oxygen[$o].o2"};for($i=0;$i -lt 6;$i++){$unit=[math]::Floor($i/3)+1;$slot=($i%3)+1;$mapping+=Mapping $oxygenAtoms[$i] "oxide[$unit].o$slot"}
$bState=@{};$oState=@{};for($b=1;$b -le 4;$b++){$bState[$b]=3};for($o=0;$o -lt 6;$o++){$oState[$o]=2}
for($u=1;$u -le 2;$u++){for($slot=1;$slot -le 2;$slot++){for($os=1;$os -le 3;$os++){$bi=(($u - 1)*2)+$slot;$oi=(($u - 1)*3)+$os;$bb=$bState[$bi];$ob=$oState[$oi - 1];$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@("subject[$bi].b",$oxygenAtoms[$oi - 1],'single');electron_contribution=[ordered]@{left=1;right=1};before=(BinaryState @(0,$bb,$bb) @(0,(4 + $ob),$ob));after=(BinaryState @(0,($bb - 1),($bb - 1)) @(0,(3 + $ob),($ob - 1)))};$bState[$bi]--;$oState[$oi - 1]--}};$atoms=@("subject[$((($u - 1)*2)+1)].b","subject[$((($u - 1)*2)+2)].b") + $oxygenAtoms[(($u - 1)*3)..((($u - 1)*3)+2)];$rewrite+=Assignment $atoms "oxide[$u]"}
$rules += New-BaseRule 'Rules.BoronOxide' 'Categories.BoronOxideElement' ([ordered]@{subject=(Role 'reactant' 'molecular' 4);oxygen=(Role 'reactant' 'molecular' 3);oxide=(Role 'product' 'molecular' 2)}) ([ordered]@{subject=(TemplateRef 'Templates.BoronElement');oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef 'Templates.BoronOxide')}) ([ordered]@{subject='Patterns.BoronElement';oxygen='Patterns.Oxygen'}) $mapping $rewrite

# Hydrogen oxidation reuses the already authored Hydrogen and Water structures.
Add-Category 'Categories.HydrogenOxideElement' @('H')
$templates += [ordered]@{representation='molecular';id='Templates.HydrogenForOxygen';parameters=[ordered]@{member=(Parameter 'Categories.HydrogenOxideElement')};atoms=@((Atom 'h1' (ParameterValue) 0 0 0),(Atom 'h2' (ParameterValue) 0 0 0));bonds=@((Bond 'h1' 'h2' 'single'));groups=@();premise_ids=$allPremises}
$applications += [ordered]@{id='HydrogenForOxygen';template='Templates.HydrogenForOxygen';arguments=[ordered]@{member='H'};formula='H2';premise_ids=$allPremises}
$patterns += [ordered]@{id='Patterns.Hydrogen';variables=[ordered]@{h1=[ordered]@{atom=[ordered]@{element='H'}};h2=[ordered]@{atom=[ordered]@{element='H'}}};relationships=@([ordered]@{kind='covalent';bond='hh';left='h1';right='h2';order='single'});premise_ids=$allPremises}
$mapping=@((Mapping 'subject[1].h1' 'oxide[1].h1'),(Mapping 'subject[1].h2' 'oxide[1].h2'),(Mapping 'oxygen[1].o1' 'oxide[1].o'),(Mapping 'subject[2].h1' 'oxide[2].h1'),(Mapping 'subject[2].h2' 'oxide[2].h2'),(Mapping 'oxygen[1].o2' 'oxide[2].o'))
$rewrite=@((Cleave-Oxygen 'oxygen[1]'))
for($h=1;$h -le 2;$h++){$rewrite += [ordered]@{kind='cleave_covalent';premise_ids=$allPremises;edge=@("subject[$h].h1","subject[$h].h2",'single');allocation='homolytic';before=(BinaryState @(0,0,0) @(0,0,0));after=(BinaryState @(0,1,1) @(0,1,1))}}
for($u=1;$u -le 2;$u++){$oxygen="oxygen[1].o$u";for($h=1;$h -le 2;$h++){$before=if($h -eq 1){@(0,6,2)}else{@(0,5,1)};$after=if($h -eq 1){@(0,5,1)}else{@(0,4,0)};$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@($oxygen,"subject[$u].h$h",'single');electron_contribution=[ordered]@{left=1;right=1};before=(BinaryState $before @(0,1,1));after=(BinaryState $after @(0,0,0))}};$rewrite += Assignment @($oxygen,"subject[$u].h1","subject[$u].h2") "oxide[$u]"}
$hydrogenRule = New-BaseRule 'Rules.HydrogenOxide' 'Categories.HydrogenOxideElement' ([ordered]@{subject=(Role 'reactant' 'molecular' 2);oxygen=(Role 'reactant' 'molecular' 1);oxide=(Role 'product' 'molecular' 2)}) ([ordered]@{subject=(TemplateRef 'Templates.HydrogenForOxygen');oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(Exact 'Water')}) ([ordered]@{subject='Patterns.Hydrogen';oxygen='Patterns.Oxygen'}) $mapping $rewrite
$hydrogenRule.premise_ids += 'premise.structure.water'
foreach($item in $hydrogenRule.cases[0].correspondence){$item.premise_ids += 'premise.structure.water'}
foreach($item in $hydrogenRule.cases[0].rewrite){$item.premise_ids += 'premise.structure.water'}
$rules += $hydrogenRule

# Phosphorus(V) oxide uses one explicit P4O10 molecule; six oxygens bridge P atoms
# and four terminal P=O bonds complete the representative Lewis structure.
Add-State P 0 2 0 3; Add-State P 0 2 2 3; Add-State P 0 3 1 2; Add-State P 0 4 2 1; Add-State P 0 5 3 0; Add-State P 0 0 0 5
Add-Category 'Categories.PhosphorusOxideElement' @('P')
$pAtoms=@();for($p=1;$p -le 4;$p++){$pAtoms+=Atom "p$p" (ParameterValue) 0 2 0}
$pBonds=@();$pEdges=@(@(1,2),@(1,3),@(1,4),@(2,3),@(2,4),@(3,4));foreach($edge in $pEdges){$pBonds+=Bond "p$($edge[0])" "p$($edge[1])" 'single'}
$templates += [ordered]@{representation='molecular';id='Templates.Phosphorus4';parameters=[ordered]@{member=(Parameter 'Categories.PhosphorusOxideElement')};atoms=$pAtoms;bonds=$pBonds;groups=@();premise_ids=$allPremises}
$poAtoms=@();for($p=1;$p -le 4;$p++){$poAtoms+=Atom "p$p" (ParameterValue) 0 0 0};for($o=1;$o -le 10;$o++){$poAtoms+=Atom "o$o" 'O' 0 4 0}
$poBonds=@();for($i=0;$i -lt 6;$i++){$edge=$pEdges[$i];$o=$i+1;$poBonds+=Bond "p$($edge[0])" "o$o" 'single';$poBonds+=Bond "p$($edge[1])" "o$o" 'single'};for($p=1;$p -le 4;$p++){$poBonds+=Bond "p$p" "o$($p+6)" 'double'}
$templates += [ordered]@{representation='molecular';id='Templates.Phosphorus5Oxide';parameters=[ordered]@{member=(Parameter 'Categories.PhosphorusOxideElement')};atoms=$poAtoms;bonds=$poBonds;groups=@();premise_ids=$allPremises}
$applications += [ordered]@{id='Phosphorus4ForOxygen';template='Templates.Phosphorus4';arguments=[ordered]@{member='P'};formula='P4';premise_ids=$allPremises},[ordered]@{id='Phosphorus5Oxide';template='Templates.Phosphorus5Oxide';arguments=[ordered]@{member='P'};formula='P4O10';premise_ids=$allPremises}
$pVariables=[ordered]@{};for($p=1;$p -le 4;$p++){$pVariables["p$p"]=[ordered]@{atom=[ordered]@{element='P'}}};$pRelationships=@();for($i=0;$i -lt 6;$i++){$edge=$pEdges[$i];$pRelationships += [ordered]@{kind='covalent';bond="pp$($i+1)";left="p$($edge[0])";right="p$($edge[1])";order='single'}}
$patterns += [ordered]@{id='Patterns.Phosphorus4';variables=$pVariables;relationships=$pRelationships;premise_ids=$allPremises}
$mapping=@();for($p=1;$p -le 4;$p++){$mapping+=Mapping "subject[1].p$p" "oxide[1].p$p"};$oxygenAtoms=@();for($o=1;$o -le 5;$o++){$oxygenAtoms+="oxygen[$o].o1","oxygen[$o].o2"};for($o=1;$o -le 10;$o++){$mapping+=Mapping $oxygenAtoms[$o-1] "oxide[1].o$o"}
$rewrite=@();$pNb=@{1=2;2=2;3=2;4=2};$pUnpaired=@{1=0;2=0;3=0;4=0}
foreach($edge in $pEdges){$l=$edge[0];$r=$edge[1];$rewrite += [ordered]@{kind='cleave_covalent';premise_ids=$allPremises;edge=@("subject[1].p$l","subject[1].p$r",'single');allocation='homolytic';before=(BinaryState @(0,$pNb[$l],$pUnpaired[$l]) @(0,$pNb[$r],$pUnpaired[$r]));after=(BinaryState @(0,($pNb[$l]+1),($pUnpaired[$l]+1)) @(0,($pNb[$r]+1),($pUnpaired[$r]+1)))};$pNb[$l]++;$pNb[$r]++;$pUnpaired[$l]++;$pUnpaired[$r]++}
for($o=1;$o -le 5;$o++){$rewrite+=Cleave-Oxygen "oxygen[$o]"}
$oxygenUse=@{};for($o=1;$o -le 10;$o++){$oxygenUse[$o]=2}
for($i=0;$i -lt 6;$i++){$edge=$pEdges[$i];$o=$i+1;foreach($p in $edge){$pb=$pUnpaired[$p];$ob=$oxygenUse[$o];$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@("subject[1].p$p",$oxygenAtoms[$o-1],'single');electron_contribution=[ordered]@{left=1;right=1};before=(BinaryState @(0,$pNb[$p],$pb) @(0,(4+$ob),$ob));after=(BinaryState @(0,($pNb[$p]-1),($pb-1)) @(0,(3+$ob),($ob-1)))};$pNb[$p]--;$pUnpaired[$p]--;$oxygenUse[$o]--}}
for($p=1;$p -le 4;$p++){$o=$p+6;$rewrite += [ordered]@{kind='reconfigure_electrons';premise_ids=$allPremises;atom="subject[1].p$p";before=@(0,2,0);after=@(0,2,2)};$rewrite += [ordered]@{kind='form_covalent';premise_ids=$allPremises;edge=@("subject[1].p$p",$oxygenAtoms[$o-1],'double');electron_contribution=[ordered]@{left=2;right=2};before=(BinaryState @(0,2,2) @(0,6,2));after=(BinaryState @(0,0,0) @(0,4,0))}}
$rewrite += Assignment (@('subject[1].p1','subject[1].p2','subject[1].p3','subject[1].p4') + $oxygenAtoms) 'oxide[1]'
$rules += New-BaseRule 'Rules.Phosphorus5Oxide' 'Categories.PhosphorusOxideElement' ([ordered]@{subject=(Role 'reactant' 'molecular' 1);oxygen=(Role 'reactant' 'molecular' 5);oxide=(Role 'product' 'molecular' 1)}) ([ordered]@{subject=(TemplateRef 'Templates.Phosphorus4');oxygen=(Exact 'Oxygen')}) ([ordered]@{oxide=(TemplateRef 'Templates.Phosphorus5Oxide')}) ([ordered]@{subject='Patterns.Phosphorus4';oxygen='Patterns.Oxygen'}) $mapping $rewrite

# Fixed-charge main-group ion pairs are generated from charge families.  The
# code below is deliberately independent of oxide identity: elemental source
# topology, charge balancing, electron transfer and ionic association are data.
$ionPairExperiences = @()
$fixedCations = [ordered]@{
    '1' = @('Li','Na','K','Rb','Cs')
    '2' = @('Be','Mg','Ca','Sr','Ba')
    '3' = @('Al')
}
$atomicNumbers = [ordered]@{Li=3;Be=4;N=7;F=9;Na=11;Mg=12;Al=13;P=15;S=16;Cl=17;K=19;Ca=20;Br=35;Rb=37;Sr=38;I=53;Cs=55;Ba=56}

function Get-Gcd([int]$a,[int]$b){while($b -ne 0){$t=$b;$b=$a%$b;$a=$t};$a}
function Get-Formula($cation,[int]$cationCount,$anion,[int]$anionCount){
    $c=if($cationCount -eq 1){''}else{"$cationCount"};$a=if($anionCount -eq 1){''}else{"$anionCount"}
    "$cation$c$anion$a"
}
function Add-FixedCationScaffold([int]$charge,$members){
    $category="Categories.FixedCation$charge";$template="Templates.FixedCation$charge`Metal";$pattern="Patterns.FixedCation$charge`Metal"
    $script:categories += [ordered]@{id=$category;subject='element';membership=[ordered]@{kind='explicit';members=$members};premise_ids=@($ionRulePremise)}
    $script:templates += [ordered]@{representation='metallic';id=$template;parameters=[ordered]@{member=(Parameter $category)};sites=@((Atom 'metal' (ParameterValue) $charge 0 0));domains=@([ordered]@{label='metallic';sites=@('metal');delocalized_electrons=$charge});premise_ids=$ionPremises}
    $script:patterns += [ordered]@{id=$pattern;variables=[ordered]@{metal=[ordered]@{atom=[ordered]@{element=(ParameterValue)}}};relationships=@([ordered]@{kind='metallic_domain';domain='metallic';sites=@('metal');delocalized_electrons=$charge});premise_ids=$ionPremises}
    foreach($member in $members){
        for($remaining=$charge;$remaining -ge 0;$remaining--){Add-State $member ($charge-$remaining) $remaining $remaining 0}
        $script:applications += [ordered]@{id="$member`FixedCation$charge`Metal";template=$template;arguments=[ordered]@{member=$member};formula=$member;premise_ids=$ionPremises}
    }
}
foreach($charge in 1..3){Add-FixedCationScaffold $charge $fixedCations["$charge"]}

function Add-ElementalAnion($id,$symbol,[int]$count,[int]$neutralNb,$bonds){
    $atoms=@();for($i=1;$i -le $count;$i++){$atoms+=Atom "a$i" $symbol 0 $neutralNb 0}
    $bondRecords=@();$relationships=@();$index=0;$bondSums=@{};for($i=1;$i -le $count;$i++){$bondSums[$i]=0}
    foreach($bond in $bonds){$index++;$delta=if($bond[2]-eq 'triple'){3}elseif($bond[2]-eq 'double'){2}else{1};$bondSums[[int]$bond[0]]+=$delta;$bondSums[[int]$bond[1]]+=$delta;$bondRecords+=Bond "a$($bond[0])" "a$($bond[1])" $bond[2];$relationships += [ordered]@{kind='covalent';bond="bond$index";left="a$($bond[0])";right="a$($bond[1])";order=$bond[2]}}
    for($i=1;$i -le $count;$i++){Add-State $symbol 0 $neutralNb 0 $bondSums[$i]}
    $script:structures += [ordered]@{representation='molecular';id=$id;premise_id=$ionStructurePremise;formula=if($count -eq 1){$symbol}else{"$symbol$count"};atoms=$atoms;bonds=$bondRecords;groups=@()}
    $variables=[ordered]@{};for($i=1;$i -le $count;$i++){$variables["a$i"]=[ordered]@{atom=[ordered]@{element=$symbol}}}
    $script:patterns += [ordered]@{id="Patterns.$id";variables=$variables;relationships=$relationships;premise_ids=$ionPremises}
}
$single=,@(1,2,'single')
$triple=,@(1,2,'triple')
$double=,@(1,2,'double')
Add-ElementalAnion 'ElementalFluorine' 'F' 2 6 $single
Add-ElementalAnion 'ElementalChlorine' 'Cl' 2 6 $single
Add-ElementalAnion 'ElementalBromine' 'Br' 2 6 $single
Add-ElementalAnion 'ElementalIodine' 'I' 2 6 $single
Add-ElementalAnion 'ElementalNitrogen' 'N' 2 2 $triple
$sulfurEdges=@();for($i=1;$i -le 8;$i++){$sulfurEdges+=,@($i,(($i%8)+1),'single')}
Add-ElementalAnion 'ElementalSulfur' 'S' 8 4 $sulfurEdges
$phosphorusEdges=@(@(1,2,'single'),@(1,3,'single'),@(1,4,'single'),@(2,3,'single'),@(2,4,'single'),@(3,4,'single'))
Add-ElementalAnion 'ElementalPhosphorus' 'P' 4 2 $phosphorusEdges

function New-IonRule($id,[int]$charge,$roles,$reactants,$products,$casePatterns,$mapping,$rewrite){
    [ordered]@{
        id=$id;parameters=[ordered]@{member=(Parameter "Categories.FixedCation$charge")};roles=$roles;reactants=$reactants
        cases=@([ordered]@{status='supported';id='charge-balanced';when=[ordered]@{kind='always'};products=$products;patterns=$casePatterns;correspondence=$mapping;rewrite=$rewrite;observation_compatibility=@([ordered]@{subject_role='salt';predicate='forms';evidence_subject='salt';premise_id=$ionObservationPremise},[ordered]@{subject_role='cation';predicate='disappears';evidence_subject='metal';premise_id=$ionObservationPremise});premise_ids=@($ionRulePremise)})
        applicability=[ordered]@{premise_id=$ionRulePremise;request_relation='contact';required_context='selected theoretical fixed-charge binary ionic outcome'}
        model_assumptions=[ordered]@{event='representative';sequence='explanatory';premise_ids=@($ionRulePremise)}
        premise_ids=@('premise.elements.iupac-periodic-table',$ionRulePremise,$ionStructurePremise,$ionValencePremise,$ionObservationPremise)+$allPremises
    }
}

function Add-IonPairFamily($name,$anionSymbol,[int]$anionCharge,$sourceId,[int]$sourceCount,[int]$neutralNb,$sourceBonds,$charges=$null){
    if($null -eq $charges){$charges=1..3}
    $anionAtomicNumber=[int]$atomicNumbers[$anionSymbol]
    foreach($charge in $charges){
        $members=$fixedCations["$charge"];$g=Get-Gcd $charge $anionCharge
        $cationPerProduct=[int]($anionCharge/$g);$anionPerProduct=[int]($charge/$g)
        $productCoefficient=[int]($sourceCount/(Get-Gcd $sourceCount $anionPerProduct))
        $sourceCoefficient=[int](($productCoefficient*$anionPerProduct)/$sourceCount)
        $metalCoefficient=$productCoefficient*$cationPerProduct
        $category="Categories.FixedCation$charge";$metalTemplate="Templates.FixedCation$charge`Metal";$metalPattern="Patterns.FixedCation$charge`Metal"
        $productTemplate="Templates.FixedCation$charge$name`Product";$ruleId="Rules.FixedCation$charge$name"
        $components=@();for($m=1;$m -le $cationPerProduct;$m++){$components += [ordered]@{label="cation$m";atoms=@((Atom 'metal' (ParameterValue) $charge 0 0));bonds=@();groups=@()}}
        for($a=1;$a -le $anionPerProduct;$a++){$components += [ordered]@{label="anion$a";atoms=@((Atom 'anion' $anionSymbol (-1*$anionCharge) 8 0));bonds=@();groups=@()}}
        $script:templates += [ordered]@{representation='ionic';id=$productTemplate;parameters=[ordered]@{member=(Parameter $category)};components=$components;associations=@([ordered]@{label='ionic';components=@($components|ForEach-Object{$_.label})});premise_ids=$ionPremises}
        foreach($member in $members){
            $formula=Get-Formula $member $cationPerProduct $anionSymbol $anionPerProduct
            $productId="$member`FixedCation$charge$name"
            $script:applications += [ordered]@{id=$productId;template=$productTemplate;arguments=[ordered]@{member=$member};formula=$formula;premise_ids=$ionPremises}
            $sourceFormula=if($sourceCount -eq 1){$anionSymbol}else{"$anionSymbol$sourceCount"}
            $equation="$(if($metalCoefficient -eq 1){''}else{"$metalCoefficient "})$member + $(if($sourceCoefficient -eq 1){''}else{"$sourceCoefficient "})$sourceFormula -> $(if($productCoefficient -eq 1){''}else{"$productCoefficient "})$formula"
            $script:ionPairExperiences += [ordered]@{slug="$($member.ToLowerInvariant())-$($anionSymbol.ToLowerInvariant())";reaction="$member`And$name";atomic_number=[int]$atomicNumbers[$member];co_atoms=@(1..$sourceCount|ForEach-Object{$anionAtomicNumber});subject_coefficient=$metalCoefficient;subject_structure="$member`FixedCation$charge`Metal";subject_formula=$member;anion_coefficient=$sourceCoefficient;anion_structure=$sourceId;anion_formula=$sourceFormula;product_coefficient=$productCoefficient;product_structure=$productId;product_formula=$formula;equation=$equation;rule=$ruleId}
        }
        $mapping=@();$rewrite=@();for($m=1;$m -le $metalCoefficient;$m++){$unit=[math]::Floor(($m-1)/$cationPerProduct)+1;$slot=(($m-1)%$cationPerProduct)+1;$mapping += [ordered]@{reactant="cation[$m].metal";product="salt[$unit].cation$slot.metal";premise_ids=$ionPremises};$rewrite += [ordered]@{kind='release_metallic';premise_ids=$ionPremises;site="cation[$m].metal";domain="cation[$m].metallic";allocation='retain_electron';before=[ordered]@{site=@($charge,0,0);domain_electrons=$charge};after=[ordered]@{site=@(0,$charge,$charge);domain_electrons=0}}}
        $anionAtoms=@();for($s=1;$s -le $sourceCoefficient;$s++){for($a=1;$a -le $sourceCount;$a++){$label=if($sourceId-eq 'Oxygen'){"o$a"}else{"a$a"};$anionAtoms+="anion[$s].$label"}}
        foreach($s in 1..$sourceCoefficient){
            $nb=@{};$u=@{};$bondSum=@{};for($a=1;$a -le $sourceCount;$a++){$nb[$a]=$neutralNb;$u[$a]=0;$bondSum[$a]=0};foreach($bond in $sourceBonds){$delta=if($bond[2]-eq 'triple'){3}elseif($bond[2]-eq 'double'){2}else{1};$bondSum[[int]$bond[0]]+=$delta;$bondSum[[int]$bond[1]]+=$delta}
            foreach($bond in $sourceBonds){$l=[int]$bond[0];$r=[int]$bond[1];$leftLabel=if($sourceId-eq 'Oxygen'){"o$l"}else{"a$l"};$rightLabel=if($sourceId-eq 'Oxygen'){"o$r"}else{"a$r"};$order=$bond[2];$delta=if($order -eq 'triple'){3}elseif($order -eq 'double'){2}else{1};Add-State $anionSymbol 0 $nb[$l] $u[$l] $bondSum[$l];Add-State $anionSymbol 0 $nb[$r] $u[$r] $bondSum[$r];$rewrite += [ordered]@{kind='cleave_covalent';premise_ids=$ionPremises;edge=@("anion[$s].$leftLabel","anion[$s].$rightLabel",$order);allocation='homolytic';before=(BinaryState @(0,$nb[$l],$u[$l]) @(0,$nb[$r],$u[$r]));after=(BinaryState @(0,($nb[$l]+$delta),($u[$l]+$delta)) @(0,($nb[$r]+$delta),($u[$r]+$delta)))};$nb[$l]+=$delta;$nb[$r]+=$delta;$u[$l]+=$delta;$u[$r]+=$delta;$bondSum[$l]-=$delta;$bondSum[$r]-=$delta;Add-State $anionSymbol 0 $nb[$l] $u[$l] $bondSum[$l];Add-State $anionSymbol 0 $nb[$r] $u[$r] $bondSum[$r]}
        }
        for($accepted=0;$accepted -le $anionCharge;$accepted++){Add-State $anionSymbol (-1*$accepted) (8-$anionCharge+$accepted) ($anionCharge-$accepted) 0}
        for($i=0;$i -lt $anionAtoms.Count;$i++){$unit=[math]::Floor($i/$anionPerProduct)+1;$slot=($i%$anionPerProduct)+1;$mapping += [ordered]@{reactant=$anionAtoms[$i];product="salt[$unit].anion$slot.anion";premise_ids=$ionPremises}}
        $donorRemaining=@();for($m=1;$m -le $metalCoefficient;$m++){$donorRemaining+=$charge};$acceptRemaining=@();for($a=0;$a -lt $anionAtoms.Count;$a++){$acceptRemaining+=$anionCharge};$di=0;$ai=0
        while($di -lt $donorRemaining.Count -and $ai -lt $acceptRemaining.Count){$count=[math]::Min($donorRemaining[$di],$acceptRemaining[$ai]);$db=$donorRemaining[$di];$ab=$anionCharge-$acceptRemaining[$ai];$da=$db-$count;$aa=$ab+$count;$rewrite += [ordered]@{kind='transfer_electron';premise_ids=$ionPremises;count=$count;donor="cation[$($di+1)].metal";acceptor=$anionAtoms[$ai];before=[ordered]@{donor=@(($charge-$db),$db,$db);acceptor=@((-1*$ab),(8-$anionCharge+$ab),($anionCharge-$ab))};after=[ordered]@{donor=@(($charge-$da),$da,$da);acceptor=@((-1*$aa),(8-$anionCharge+$aa),($anionCharge-$aa))}};$donorRemaining[$di]-=$count;$acceptRemaining[$ai]-=$count;if($donorRemaining[$di]-eq 0){$di++};if($acceptRemaining[$ai]-eq 0){$ai++}}
        for($unit=1;$unit -le $productCoefficient;$unit++){$atoms=@();$groups=@();$componentCharges=@();for($m=1;$m -le $cationPerProduct;$m++){$idx=(($unit-1)*$cationPerProduct)+$m;$atoms+="cation[$idx].metal";$groups+=,@("cation[$idx].metal");$componentCharges+=$charge};for($a=1;$a -le $anionPerProduct;$a++){$idx=(($unit-1)*$anionPerProduct)+$a;$atoms+=$anionAtoms[$idx-1];$groups+=,@($anionAtoms[$idx-1]);$componentCharges+=(-1*$anionCharge)};$rewrite += [ordered]@{kind='associate_ionic';premise_ids=$ionPremises;label="ionic.product$unit";components=$groups;component_charges=$componentCharges};$rewrite+=@{kind='assign_product';premise_ids=$ionPremises;atoms=$atoms;product="salt[$unit]"}}
        $roles=[ordered]@{cation=(Role 'reactant' 'metallic' $metalCoefficient);anion=(Role 'reactant' 'molecular' $sourceCoefficient);salt=(Role 'product' 'ionic' $productCoefficient)}
        $script:rules += New-IonRule $ruleId $charge $roles ([ordered]@{cation=(TemplateRef $metalTemplate);anion=(Exact $sourceId)}) ([ordered]@{salt=(TemplateRef $productTemplate)}) ([ordered]@{cation=$metalPattern;anion="Patterns.$sourceId"}) $mapping $rewrite
    }
}

foreach($anion in @(@('Fluoride','F','ElementalFluorine',2,6,$single),@('Chloride','Cl','ElementalChlorine',2,6,$single),@('Bromide','Br','ElementalBromine',2,6,$single),@('Iodide','I','ElementalIodine',2,6,$single))){Add-IonPairFamily $anion[0] $anion[1] 1 $anion[2] $anion[3] $anion[4] $anion[5]}
Add-IonPairFamily 'Sulfide' 'S' 2 'ElementalSulfur' 8 4 $sulfurEdges
Add-IonPairFamily 'Nitride' 'N' 3 'ElementalNitrogen' 2 2 $triple
Add-IonPairFamily 'Phosphide' 'P' 3 'ElementalPhosphorus' 4 2 $phosphorusEdges
# Normal oxides already exist for Li, all +2 metals and Al.  This adds the
# missing +1 normal-oxide alternatives without duplicating those experiences.
Add-IonPairFamily 'NormalOxide' 'O' 2 'Oxygen' 2 4 $double @(1)
$ionPairExperiences = @($ionPairExperiences | Where-Object { !($_.product_formula -eq 'Li2O') })

$candidate=[ordered]@{
    schema_version=1;id='oxygen-reactions'
    evidence=@([ordered]@{id='evidence.openstax.oxygen-compounds';title='Chemistry: Atoms First 2e';publisher='OpenStax';locator='Occurrence, Preparation, and Compounds of Oxygen';reference='https://openstax.org/books/chemistry-atoms-first-2e/pages/18-9-occurrence-preparation-and-compounds-of-oxygen';retrieved_on='2026-07-15';usage='Representative normal oxide, peroxide, superoxide, and covalent oxide outcomes'},[ordered]@{id='evidence.openstax.ionic-compounds';title='Chemistry 2e';publisher='OpenStax';locator='Ionic Bonding';reference='https://openstax.org/books/chemistry-2e/pages/7-1-ionic-bonding';retrieved_on='2026-07-15';usage='Charge neutrality, electron transfer, fixed-charge monatomic ions, and binary ionic formula units'})
    premises=@((Premise $rulePremise 'The listed element families have the representative balanced oxygen outcomes encoded by their supported cases.'),(Premise $structurePremise 'Oxygen and oxide products use explicit localized or delocalized bonds, formal charges, ionic components, and representative network fragments.'),(Premise $valencePremise 'The listed electron states are the closed valence domain used by the oxygen reaction operations.'),(Premise $observationPremise 'Formation of the oxide product and disappearance of the reactant are compatible generic observations for these representative theoretical experiences.'),[ordered]@{id=$ionRulePremise;statement='Fixed-charge binary ionic formula units use the smallest whole-number cation-to-anion ratio whose component charges sum to zero.';evidence=@('evidence.openstax.ionic-compounds');review=[ordered]@{status='provisional';reviewers=@()};rule_version='1'},[ordered]@{id=$ionStructurePremise;statement='A binary ionic formula unit is represented by explicitly charged monatomic components in a charge-aware ionic association.';evidence=@('evidence.openstax.ionic-compounds');review=[ordered]@{status='provisional';reviewers=@()};rule_version='1'},[ordered]@{id=$ionValencePremise;statement='The fixed-charge ion-pair domain transfers the cation charge to anion valence vacancies after explanatory elemental-bond cleavage.';evidence=@('evidence.openstax.ionic-compounds');review=[ordered]@{status='provisional';reviewers=@()};rule_version='1'},[ordered]@{id=$ionObservationPremise;statement='Formation of the binary salt and disappearance of the selected elemental reactants are compatible generic theoretical observations.';evidence=@('evidence.openstax.ionic-compounds');review=[ordered]@{status='provisional';reviewers=@()};rule_version='1'})
    valence_premises=@([ordered]@{premise_id=$valencePremise;neutral_valence=@($elements.GetEnumerator()|ForEach-Object{[ordered]@{element=$_.Key;neutral_valence_electrons=$_.Value}});supported_states=@($states.Values);metallic_domain_states=@(@('Li','Be','Na','Mg','Al','K','Ca','Rb','Sr','Cs','Ba'|ForEach-Object{$e=$_;$q=if($e -in @('Li','Na','K','Rb','Cs')){1}elseif($e -eq 'Al'){3}else{2};[ordered]@{element=$e;site_formal_charge=$q;site_local_electrons=0;delocalized_electrons_per_site=$q}}) + @($transitionMetallicStates))},[ordered]@{premise_id=$ionValencePremise;neutral_valence=@($elements.GetEnumerator()|Where-Object{$_.Key -in @('Li','Na','K','Rb','Cs','Be','Mg','Ca','Sr','Ba','Al','F','Cl','Br','I','O','S','N','P')}|ForEach-Object{[ordered]@{element=$_.Key;neutral_valence_electrons=$_.Value}});supported_states=@($states.Values|Where-Object{$_.element -in @('Li','Na','K','Rb','Cs','Be','Mg','Ca','Sr','Ba','Al','F','Cl','Br','I','O','S','N','P')});metallic_domain_states=@('Li','Na','K','Rb','Cs','Be','Mg','Ca','Sr','Ba','Al'|ForEach-Object{$e=$_;$q=if($e -in @('Li','Na','K','Rb','Cs')){1}elseif($e -eq 'Al'){3}else{2};[ordered]@{element=$e;site_formal_charge=$q;site_local_electrons=0;delocalized_electrons_per_site=$q}})})
    structures=$structures;rules=@();elements=@();element_categories=$categories;structural_traits=@();structure_templates=$templates;structure_applications=$applications;graph_patterns=$patterns;generalized_rules=$rules
}

$evidence=[ordered]@{schema_version=1;id='Evidence.OxygenReaction@1';claims=@([ordered]@{id='R1';subject_role='product';subject='oxide';predicate='forms';sources=@('S1')},[ordered]@{id='R2';subject_role='reactant';subject='element';predicate='disappears';sources=@('S1')});sources=@([ordered]@{id='S1';title='Occurrence, Preparation, and Compounds of Oxygen';publisher='OpenStax';url='https://openstax.org/books/chemistry-atoms-first-2e/pages/18-9-occurrence-preparation-and-compounds-of-oxygen';supports=@('R1','R2')})}

Write-Utf8 (Join-Path $candidateDir 'candidate.json') ($candidate | ConvertTo-Json -Depth 100)
Write-Utf8 (Join-Path $candidateDir 'evidence.json') ($evidence | ConvertTo-Json -Depth 20)

$experiences=@(
    @('hydrogen-oxygen','HydrogenAndOxygen','2','HydrogenForOxygen','H2','molecular','2','Water','H2O','molecular','2 H2 + O2 -> 2 H2O','Rules.HydrogenOxide'),
    @('lithium-oxygen','LithiumAndOxygen','4','LiMetalForOxygen','Li','metallic','2','LiMonovalentNormalOxide','Li2O','ionic','4 Li + O2 -> 2 Li2O','Rules.MonovalentNormalOxide'),
    @('beryllium-oxygen','BerylliumAndOxygen','2','BeMetalForOxygen','Be','metallic','2','BeDivalentNormalOxide','BeO','ionic','2 Be + O2 -> 2 BeO','Rules.DivalentNormalOxide'),
    @('boron-oxygen','BoronAndOxygen','4','BForOxygen','B','molecular','2','BBoronOxide','B2O3','molecular','4 B + 3 O2 -> 2 B2O3','Rules.BoronOxide'),
    @('carbon-oxygen','CarbonAndOxygen','1','CForOxygen','C','molecular','1','CGroup14Dioxide','CO2','molecular','C + O2 -> CO2','Rules.Group14Dioxide'),
    @('sodium-oxygen','SodiumAndOxygen','2','NaMetalForOxygen','Na','metallic','1','NaMonovalentPeroxide','Na2O2','ionic','2 Na + O2 -> Na2O2','Rules.MonovalentPeroxide'),
    @('magnesium-oxygen','MagnesiumAndOxygen','2','MgMetalForOxygen','Mg','metallic','2','MgDivalentNormalOxide','MgO','ionic','2 Mg + O2 -> 2 MgO','Rules.DivalentNormalOxide'),
    @('aluminium-oxygen','AluminiumAndOxygen','4','AlMetalForOxygen','Al','metallic','2','AlTrivalentNormalOxide','Al2O3','ionic','4 Al + 3 O2 -> 2 Al2O3','Rules.TrivalentNormalOxide'),
    @('silicon-oxygen','SiliconAndOxygen','1','SiForOxygen','Si','molecular','1','SiGroup14Dioxide','SiO2','molecular','Si + O2 -> SiO2','Rules.Group14Dioxide'),
    @('phosphorus-oxygen','PhosphorusAndOxygen','1','Phosphorus4ForOxygen','P4','molecular','1','Phosphorus5Oxide','P4O10','molecular','P4 + 5 O2 -> P4O10','Rules.Phosphorus5Oxide'),
    @('sulfur-oxygen','SulfurAndOxygen','1','SForOxygen','S','molecular','1','SSulfurDioxide','SO2','molecular','S + O2 -> SO2','Rules.SulfurDioxide'),
    @('potassium-oxygen','PotassiumAndOxygen','1','KMetalForOxygen','K','metallic','1','KSuperoxide','KO2','ionic','K + O2 -> KO2','Rules.Superoxide'),
    @('calcium-oxygen','CalciumAndOxygen','2','CaMetalForOxygen','Ca','metallic','2','CaDivalentNormalOxide','CaO','ionic','2 Ca + O2 -> 2 CaO','Rules.DivalentNormalOxide'),
    @('rubidium-oxygen','RubidiumAndOxygen','1','RbMetalForOxygen','Rb','metallic','1','RbSuperoxide','RbO2','ionic','Rb + O2 -> RbO2','Rules.Superoxide'),
    @('strontium-oxygen','StrontiumAndOxygen','2','SrMetalForOxygen','Sr','metallic','2','SrDivalentNormalOxide','SrO','ionic','2 Sr + O2 -> 2 SrO','Rules.DivalentNormalOxide'),
    @('caesium-oxygen','CaesiumAndOxygen','1','CsMetalForOxygen','Cs','metallic','1','CsSuperoxide','CsO2','ionic','Cs + O2 -> CsO2','Rules.Superoxide'),
    @('barium-oxygen','BariumAndOxygen','2','BaMetalForOxygen','Ba','metallic','2','BaDivalentNormalOxide','BaO','ionic','2 Ba + O2 -> 2 BaO','Rules.DivalentNormalOxide')
) + $transitionExperiences
foreach($x in $experiences){
    $id=$x[0];$reaction=$x[1];$subjectCoeff=$x[2];$subjectStructure=$x[3];$subjectFormula=$x[4];$subjectRep=$x[5];$productCoeff=$x[6];$productStructure=$x[7];$productFormula=$x[8];$productRep=$x[9];$equation=$x[10];$rule=$x[11]
    $oxygenCoeff=if($equation -match '\+ (?:(\d+) )?O2') { if($Matches[1]){[int]$Matches[1]}else{1} } else { throw "Cannot read oxygen coefficient from $equation" }
    $source=@"
chems 1
use catalog ChemSpec.Theoretical@1
reaction $reaction where
  reactants
    subject := $subjectCoeff of $subjectStructure
    oxygen := $oxygenCoeff of Oxygen
  products
    oxide := $productCoeff of $productStructure
  equation
    $subjectCoeff $subjectFormula`[$subjectRep`] + $oxygenCoeff O2[molecular]
    -> $productCoeff $productFormula`[$productRep`]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.OxygenReaction@1
    product oxide forms claim R1
    reactant subject disappears claim R2
  by
    apply $rule
      subject := subject
      oxygen := oxygen
      oxide := oxide
"@.TrimStart()
    Write-Utf8 (Join-Path $experienceDir "oxygen-$id-001.chems") ($source.TrimEnd() + "`n")
    Write-Utf8 (Join-Path $observationDir "oxygen-$id-001.evidence.json") ($evidence | ConvertTo-Json -Depth 20)
}

$ionEvidence=[ordered]@{schema_version=1;id='Evidence.IonPairReaction@1';claims=@([ordered]@{id='R1';subject_role='product';subject='salt';predicate='forms';sources=@('S1')},[ordered]@{id='R2';subject_role='reactant';subject='metal';predicate='disappears';sources=@('S1')});sources=@([ordered]@{id='S1';title='Ionic Bonding';publisher='OpenStax';url='https://openstax.org/books/chemistry-2e/pages/7-1-ionic-bonding';supports=@('R1','R2')})}
foreach($x in $ionPairExperiences){
    $source=@"
chems 1
use catalog ChemSpec.Theoretical@1
reaction $($x.reaction) where
  reactants
    cation := $($x.subject_coefficient) of $($x.subject_structure)
    anion := $($x.anion_coefficient) of $($x.anion_structure)
  products
    salt := $($x.product_coefficient) of $($x.product_structure)
  equation
    $($x.subject_coefficient) $($x.subject_formula)[metallic] + $($x.anion_coefficient) $($x.anion_formula)[molecular]
    -> $($x.product_coefficient) $($x.product_formula)[ionic]
  model
    event := representative
    sequence := explanatory
  observe from Evidence.IonPairReaction@1
    product salt forms claim R1
    reactant cation disappears claim R2
  by
    apply $($x.rule)
      cation := cation
      anion := anion
      salt := salt
"@.TrimStart()
    Write-Utf8 (Join-Path $experienceDir "ionpair-$($x.slug)-001.chems") ($source.TrimEnd()+"`n")
    Write-Utf8 (Join-Path $observationDir "ionpair-$($x.slug)-001.evidence.json") ($ionEvidence|ConvertTo-Json -Depth 20)
}

# Register candidate experiences beside trusted ones without compiling them
# into the application before independent catalogue promotion.
$registryPath = Join-Path $Root 'catalogue/experience-registry.json'
$registry = Get-Content -Raw -Encoding utf8 $registryPath | ConvertFrom-Json
$oxygenStatus = if($registry.experiences | Where-Object { $_.id -like 'oxygen-*' -and $_.status -eq 'trusted' }){'trusted'}else{'candidate'}
$trustedCataloguePath=Join-Path $Root 'catalogue/trusted/periodic-table-and-alkali-water/catalogue.json'
$trustedHasIonPairs=(Test-Path $trustedCataloguePath) -and ((Get-Content -Raw -Encoding utf8 $trustedCataloguePath) -match 'Rules.FixedCation1Fluoride')
$ionStatus = if($trustedHasIonPairs -or ($registry.experiences | Where-Object { $_.id -like 'ionpair-*' -and $_.status -eq 'trusted' })){'trusted'}else{'candidate'}
$baseExperiences = @($registry.experiences | Where-Object { $_.id -notlike 'oxygen-*' -and $_.id -notlike 'ionpair-*' })
foreach($record in $baseExperiences){if($null -eq $record.status){$record | Add-Member -NotePropertyName status -NotePropertyValue trusted};$record.PSObject.Properties.Remove('name')}
$slugs = [ordered]@{'1'='hydrogen-oxygen';'3'='lithium-oxygen';'4'='beryllium-oxygen';'5'='boron-oxygen';'6'='carbon-oxygen';'11'='sodium-oxygen';'12'='magnesium-oxygen';'13'='aluminium-oxygen';'14'='silicon-oxygen';'15'='phosphorus-oxygen';'16'='sulfur-oxygen';'19'='potassium-oxygen';'20'='calcium-oxygen';'37'='rubidium-oxygen';'38'='strontium-oxygen';'55'='caesium-oxygen';'56'='barium-oxygen'}
$elementCatalogue = Get-Content -Raw -Encoding utf8 (Join-Path $Root 'catalogue/candidates/periodic-table-and-alkali-water/candidate.json') | ConvertFrom-Json
$screening = Get-Content -Raw -Encoding utf8 (Join-Path $Root 'catalogue/oxygen-screening/oxygen.json') | ConvertFrom-Json
$candidateExperiences = @()
foreach($screened in $screening.element_outcomes){
    if($screened.outcome.kind -ne 'representative'){continue}
    $atomicNumber = [int]$screened.atomic_number
    $slug = $slugs["$atomicNumber"]
    $element = $elementCatalogue.elements | Where-Object atomic_number -eq $atomicNumber | Select-Object -First 1
    $candidateExperiences += [ordered]@{
        id="oxygen-$slug";status=$oxygenStatus;atomic_number=$atomicNumber;co_reactant_atoms=@(8,8)
        source_path="conformance/end-to-end/oxygen-$slug-001.chems"
        evidence_path="conformance/observations/oxygen-$slug-001.evidence.json"
        request="What happens when $($element.name.ToLowerInvariant()) reacts with oxygen?"
        equation=$screened.outcome.equation;subject_name=$element.name.ToLowerInvariant()
    }
}
foreach($transition in $transitionExperiences){
    $slug=$transition[0];$atomicNumber=[int]$transition[12];$equation=$transition[10]
    $element=$elementCatalogue.elements|Where-Object atomic_number -eq $atomicNumber|Select-Object -First 1
    $candidateExperiences += [ordered]@{id="oxygen-$slug";status=$oxygenStatus;atomic_number=$atomicNumber;co_reactant_atoms=@(8,8);source_path="conformance/end-to-end/oxygen-$slug-001.chems";evidence_path="conformance/observations/oxygen-$slug-001.evidence.json";request="What happens when $($element.name.ToLowerInvariant()) reacts with oxygen for this reviewed product outcome?";equation=$equation;subject_name=$element.name.ToLowerInvariant()}
}
$ionExperiences=@()
foreach($x in $ionPairExperiences){
    $member=$elementCatalogue.elements|Where-Object atomic_number -eq $x.atomic_number|Select-Object -First 1
    $ionExperiences += [ordered]@{id="ionpair-$($x.slug)";status=$ionStatus;atomic_number=$x.atomic_number;co_reactant_atoms=$x.co_atoms;source_path="conformance/end-to-end/ionpair-$($x.slug)-001.chems";evidence_path="conformance/observations/ionpair-$($x.slug)-001.evidence.json";request="What fixed-charge ionic compound forms when $($member.name.ToLowerInvariant()) reacts with $($x.anion_formula)?";equation=$x.equation;subject_name=$member.name.ToLowerInvariant()}
}
$registry.experiences = @($baseExperiences) + $candidateExperiences + $ionExperiences
Write-Utf8 $registryPath ($registry | ConvertTo-Json -Depth 20)
Copy-Item (Join-Path $experienceDir 'oxygen-potassium-oxygen-001.chems') (Join-Path $candidateDir 'example.chems') -Force

Write-Host "Generated $($rules.Count) reusable oxygen/ion-pair rules, $($experiences.Count) oxygen experiences, and $($ionPairExperiences.Count) ion-pair experiences."
